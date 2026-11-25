#[cfg(windows)]
mod win_impl {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use anyhow::{anyhow, Result};
    use chrono::Local;
    use regex::Regex;
    use serde_json::Value;
    use tokio::sync::{mpsc::UnboundedSender, oneshot};
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2Controller,
        ICoreWebView2Environment, ICoreWebView2EnvironmentOptions,
        COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
    };
    use webview2_com::{
        CapturePreviewCompletedHandler, CreateCoreWebView2ControllerCompletedHandler,
        CreateCoreWebView2EnvironmentCompletedHandler, ExecuteScriptCompletedHandler,
        NavigationCompletedEventHandler,
    };
    use windows::core::{w, HRESULT, PCWSTR};
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::HBRUSH;
    use windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, IStream, COINIT_APARTMENTTHREADED, STREAM_SEEK_SET,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::SHCreateMemStream;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
        GetMessageW, LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassExW, ShowWindow,
        TranslateMessage, UpdateWindow, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW, MSG,
        SW_SHOW, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_SIZE, WM_USER, WNDCLASSEXW,
        WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };

    use crate::engine::browser_runner::{
        append_file_log, emit_log, emit_status, resolve_step_date, ExecutionRequest,
        ExecutionStepInput, RunnerEvent,
    };
    use crate::engine::param_resolver::{ParamResolver, RocDateUtils};
    use crate::models::GeneratedScheduleItem;

    const WM_APP_CMD: u32 = WM_USER + 101;

    enum WebViewCmd {
        Navigate {
            url: String,
            reply: oneshot::Sender<Result<()>>,
        },
        EvalJs {
            script: String,
            reply: oneshot::Sender<Result<Value>>,
        },
        CapturePng {
            file_path: String,
            reply: oneshot::Sender<Result<String>>,
        },
        Close,
    }

    struct WebViewHandle {
        hwnd_raw: usize,
        cmd_tx: mpsc::Sender<WebViewCmd>,
    }

    struct StaContext {
        hwnd: HWND,
        cmd_rx: mpsc::Receiver<WebViewCmd>,
        cmd_tx_for_init: mpsc::Sender<WebViewCmd>,
        controller: Option<ICoreWebView2Controller>,
        webview: Option<ICoreWebView2>,
        init_tx: Option<oneshot::Sender<Result<WebViewHandle>>>,
        pending_nav: Option<oneshot::Sender<Result<()>>>,
        nav_token: i64,
    }

    thread_local! {
        static STA_CTX: RefCell<Option<StaContext>> = const { RefCell::new(None) };
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_SIZE => {
                STA_CTX.with(|cell| {
                    if let Some(ctx) = cell.borrow().as_ref() {
                        if let Some(ctrl) = ctx.controller.as_ref() {
                            let mut rect = RECT::default();
                            let _ = GetClientRect(hwnd, &mut rect);
                            let _ = ctrl.SetBounds(rect);
                        }
                    }
                });
                LRESULT(0)
            }
            WM_APP_CMD => {
                process_pending_commands();
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                STA_CTX.with(|cell| {
                    if let Some(mut ctx) = cell.borrow_mut().take() {
                        if let Some(tx) = ctx.init_tx.take() {
                            let _ = tx.send(Err(anyhow!("WebView2 視窗在初始化完成前被關閉")));
                        }
                        if let Some(tx) = ctx.pending_nav.take() {
                            let _ = tx.send(Err(anyhow!("WebView2 視窗已關閉")));
                        }
                        while let Ok(cmd) = ctx.cmd_rx.try_recv() {
                            match cmd {
                                WebViewCmd::Navigate { reply, .. } => {
                                    let _ = reply.send(Err(anyhow!("WebView2 視窗已關閉")));
                                }
                                WebViewCmd::EvalJs { reply, .. } => {
                                    let _ = reply.send(Err(anyhow!("WebView2 視窗已關閉")));
                                }
                                WebViewCmd::CapturePng { reply, .. } => {
                                    let _ = reply.send(Err(anyhow!("WebView2 視窗已關閉")));
                                }
                                WebViewCmd::Close => {}
                            }
                        }
                        if let Some(ctrl) = ctx.controller.take() {
                            let _ = ctrl.Close();
                        }
                        ctx.webview = None;
                    }
                });
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    fn process_pending_commands() {
        let mut cmds = Vec::new();
        STA_CTX.with(|cell| {
            if let Some(ctx) = cell.borrow_mut().as_mut() {
                while let Ok(cmd) = ctx.cmd_rx.try_recv() {
                    cmds.push(cmd);
                }
            }
        });

        for cmd in cmds {
            match cmd {
                WebViewCmd::Navigate { url, reply } => {
                    STA_CTX.with(|cell| {
                        let mut guard = cell.borrow_mut();
                        let Some(ctx) = guard.as_mut() else {
                            let _ = reply.send(Err(anyhow!("WebView2 已關閉")));
                            return;
                        };
                        let Some(webview) = ctx.webview.clone() else {
                            let _ = reply.send(Err(anyhow!("WebView2 尚未初始化")));
                            return;
                        };
                        if let Some(prev) = ctx.pending_nav.take() {
                            let _ = prev.send(Err(anyhow!("導航被新的請求取代")));
                        }
                        ctx.pending_nav = Some(reply);
                        let wide = to_wide(&url);
                        let nav_res = unsafe { webview.Navigate(PCWSTR(wide.as_ptr())) };
                        if let Err(e) = nav_res {
                            if let Some(tx) = ctx.pending_nav.take() {
                                let _ = tx.send(Err(anyhow!("WebView2 Navigate 失敗 ({url}): {e}")));
                            }
                        }
                    });
                }
                WebViewCmd::EvalJs { script, reply } => {
                    let webview_opt =
                        STA_CTX.with(|cell| cell.borrow().as_ref().and_then(|c| c.webview.clone()));
                    let Some(webview) = webview_opt else {
                        let _ = reply.send(Err(anyhow!("WebView2 尚未就緒或已關閉")));
                        continue;
                    };

                    let wide = to_wide(&script);
                    let mut reply_opt = Some(reply);
                    let handler = ExecuteScriptCompletedHandler::create(Box::new(
                        move |error_code, result_json| {
                            if let Some(tx) = reply_opt.take() {
                                match error_code {
                                    Ok(()) => match serde_json::from_str::<Value>(&result_json) {
                                        Ok(val) => {
                                            let _ = tx.send(Ok(val));
                                        }
                                        Err(e) => {
                                            let _ = tx.send(Err(anyhow!(
                                                "解析 JS 回傳 JSON 失敗: {e} | raw={result_json}"
                                            )));
                                        }
                                    },
                                    Err(e) => {
                                        let _ = tx.send(Err(anyhow!("ExecuteScript 執行錯誤: {e}")));
                                    }
                                }
                            }
                            Ok(())
                        },
                    ));

                    let call_res = unsafe { webview.ExecuteScript(PCWSTR(wide.as_ptr()), &handler) };
                    if let Err(e) = call_res {
                        append_file_log("WARN", &format!("ExecuteScript COM 呼叫失敗: {e}"));
                    }
                }
                WebViewCmd::CapturePng { file_path, reply } => {
                    let webview_opt =
                        STA_CTX.with(|cell| cell.borrow().as_ref().and_then(|c| c.webview.clone()));
                    let Some(webview) = webview_opt else {
                        let _ = reply.send(Err(anyhow!("WebView2 尚未就緒，無法截圖")));
                        continue;
                    };

                    let stream_opt: Option<IStream> = unsafe { SHCreateMemStream(None) };
                    let Some(stream) = stream_opt else {
                        let _ = reply.send(Err(anyhow!("建立記憶體串流 SHCreateMemStream 失敗")));
                        continue;
                    };

                    let mut reply_opt = Some(reply);
                    let stream_for_handler = stream.clone();
                    let path_for_handler = file_path.clone();
                    let handler = CapturePreviewCompletedHandler::create(Box::new(move |res| {
                        if let Some(tx) = reply_opt.take() {
                            if let Err(e) = res {
                                let _ = tx.send(Err(anyhow!("CapturePreview 失敗: {e}")));
                                return Ok(());
                            }
                            let mut png_bytes = Vec::new();
                            unsafe {
                                if let Err(e) = stream_for_handler.Seek(0, STREAM_SEEK_SET, None) {
                                    let _ = tx.send(Err(anyhow!("IStream Seek 失敗: {e}")));
                                    return Ok(());
                                }
                                let mut buf = [0u8; 8192];
                                loop {
                                    let mut bytes_read = 0u32;
                                    let hr: HRESULT = stream_for_handler.Read(
                                        buf.as_mut_ptr().cast(),
                                        buf.len() as u32,
                                        Some(&mut bytes_read),
                                    );
                                    if hr.is_err() {
                                        let _ = tx.send(Err(anyhow!("IStream Read 失敗: {hr:?}")));
                                        return Ok(());
                                    }
                                    if bytes_read == 0 {
                                        break;
                                    }
                                    png_bytes.extend_from_slice(&buf[..bytes_read as usize]);
                                }
                            }
                            match std::fs::write(&path_for_handler, &png_bytes) {
                                Ok(()) => {
                                    let _ = tx.send(Ok(path_for_handler));
                                }
                                Err(e) => {
                                    let _ = tx.send(Err(anyhow!("寫入截圖檔案失敗: {e}")));
                                }
                            }
                        }
                        Ok(())
                    }));

                    let cap_res = unsafe {
                        webview.CapturePreview(
                            COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
                            &stream,
                            &handler,
                        )
                    };
                    if let Err(e) = cap_res {
                        append_file_log("WARN", &format!("CapturePreview COM 呼叫失敗: {e}"));
                    }
                }
                WebViewCmd::Close => {
                    let hwnd_opt = STA_CTX.with(|cell| cell.borrow().as_ref().map(|c| c.hwnd));
                    if let Some(hwnd) = hwnd_opt {
                        unsafe {
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                }
            }
        }
    }

    fn run_sta_webview_thread(
        user_data_dir: String,
        init_tx: oneshot::Sender<Result<WebViewHandle>>,
    ) {
        unsafe {
            let co_hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if co_hr.is_err() {
                let _ = init_tx.send(Err(anyhow!("CoInitializeEx 失敗: {co_hr:?}")));
                return;
            }
        }

        let (cmd_tx, cmd_rx) = mpsc::channel::<WebViewCmd>();
        let class_name = w!("YcmWebView2AutomationWindow");
        let window_title = w!("YCM 自動化執行瀏覽器 (Edge WebView2)");

        let hwnd = unsafe {
            let hmodule = match GetModuleHandleW(None) {
                Ok(m) => m,
                Err(e) => {
                    let _ = init_tx.send(Err(anyhow!("GetModuleHandleW 失敗: {e}")));
                    CoUninitialize();
                    return;
                }
            };
            let hinstance = HINSTANCE(hmodule.0);
            let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();

            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: Default::default(),
                hCursor: cursor,
                hbrBackground: HBRUSH::default(),
                lpszMenuName: PCWSTR::null(),
                lpszClassName: class_name,
                hIconSm: Default::default(),
            };
            // Ignore ERROR_CLASS_ALREADY_EXISTS across runs
            let _ = RegisterClassExW(&wc);

            match CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name,
                window_title,
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                1280,
                860,
                None,
                None,
                Some(hinstance),
                None,
            ) {
                Ok(h) => h,
                Err(e) => {
                    let _ = init_tx.send(Err(anyhow!("CreateWindowExW 建立視窗失敗: {e}")));
                    CoUninitialize();
                    return;
                }
            }
        };

        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = UpdateWindow(hwnd);
        }

        STA_CTX.with(|cell| {
            *cell.borrow_mut() = Some(StaContext {
                hwnd,
                cmd_rx,
                cmd_tx_for_init: cmd_tx,
                controller: None,
                webview: None,
                init_tx: Some(init_tx),
                pending_nav: None,
                nav_token: 0,
            });
        });

        let user_data_wide = to_wide(&user_data_dir);
        let env_handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
            move |env_res, created_env| {
                let env: ICoreWebView2Environment = match (env_res, created_env) {
                    (Ok(()), Some(e)) => e,
                    (Err(e), _) => {
                        STA_CTX.with(|cell| {
                            if let Some(ctx) = cell.borrow_mut().as_mut() {
                                if let Some(tx) = ctx.init_tx.take() {
                                    let _ = tx.send(Err(anyhow!(
                                        "建立 Edge WebView2 Environment 失敗 ({e})；請確認本機已安裝 Microsoft Edge WebView2 Runtime"
                                    )));
                                }
                            }
                        });
                        unsafe {
                            let _ = DestroyWindow(hwnd);
                        }
                        return Ok(());
                    }
                    (Ok(()), None) => {
                        STA_CTX.with(|cell| {
                            if let Some(ctx) = cell.borrow_mut().as_mut() {
                                if let Some(tx) = ctx.init_tx.take() {
                                    let _ = tx.send(Err(anyhow!(
                                        "WebView2 Environment 回傳為空"
                                    )));
                                }
                            }
                        });
                        unsafe {
                            let _ = DestroyWindow(hwnd);
                        }
                        return Ok(());
                    }
                };

                let ctrl_handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
                    move |ctrl_res, created_ctrl| {
                        let controller: ICoreWebView2Controller = match (ctrl_res, created_ctrl) {
                            (Ok(()), Some(c)) => c,
                            (Err(e), _) => {
                                STA_CTX.with(|cell| {
                                    if let Some(ctx) = cell.borrow_mut().as_mut() {
                                        if let Some(tx) = ctx.init_tx.take() {
                                            let _ = tx.send(Err(anyhow!(
                                                "建立 WebView2 Controller 失敗: {e}"
                                            )));
                                        }
                                    }
                                });
                                unsafe {
                                    let _ = DestroyWindow(hwnd);
                                }
                                return Ok(());
                            }
                            (Ok(()), None) => {
                                STA_CTX.with(|cell| {
                                    if let Some(ctx) = cell.borrow_mut().as_mut() {
                                        if let Some(tx) = ctx.init_tx.take() {
                                            let _ = tx.send(Err(anyhow!(
                                                "WebView2 Controller 回傳為空"
                                            )));
                                        }
                                    }
                                });
                                unsafe {
                                    let _ = DestroyWindow(hwnd);
                                }
                                return Ok(());
                            }
                        };

                        unsafe {
                            let mut rect = RECT::default();
                            let _ = GetClientRect(hwnd, &mut rect);
                            let _ = controller.SetBounds(rect);
                        }

                        let webview = match unsafe { controller.CoreWebView2() } {
                            Ok(wv) => wv,
                            Err(e) => {
                                STA_CTX.with(|cell| {
                                    if let Some(ctx) = cell.borrow_mut().as_mut() {
                                        if let Some(tx) = ctx.init_tx.take() {
                                            let _ = tx.send(Err(anyhow!(
                                                "取得 ICoreWebView2 介面失敗: {e}"
                                            )));
                                        }
                                    }
                                });
                                unsafe {
                                    let _ = DestroyWindow(hwnd);
                                }
                                return Ok(());
                            }
                        };

                        let nav_handler = NavigationCompletedEventHandler::create(Box::new(
                            move |_sender, _args| {
                                STA_CTX.with(|cell| {
                                    if let Some(ctx) = cell.borrow_mut().as_mut() {
                                        if let Some(tx) = ctx.pending_nav.take() {
                                            let _ = tx.send(Ok(()));
                                        }
                                    }
                                });
                                Ok(())
                            },
                        ));

                        let mut token = 0i64;
                        let _ = unsafe { webview.add_NavigationCompleted(&nav_handler, &mut token) };

                        STA_CTX.with(|cell| {
                            if let Some(ctx) = cell.borrow_mut().as_mut() {
                                ctx.controller = Some(controller);
                                ctx.webview = Some(webview);
                                ctx.nav_token = token;
                                if let Some(tx) = ctx.init_tx.take() {
                                    let _ = tx.send(Ok(WebViewHandle {
                                        hwnd_raw: hwnd.0 as usize,
                                        cmd_tx: ctx.cmd_tx_for_init.clone(),
                                    }));
                                }
                            }
                        });

                        Ok(())
                    },
                ));

                if let Err(e) = unsafe { env.CreateCoreWebView2Controller(hwnd, &ctrl_handler) } {
                    STA_CTX.with(|cell| {
                        if let Some(ctx) = cell.borrow_mut().as_mut() {
                            if let Some(tx) = ctx.init_tx.take() {
                                let _ = tx.send(Err(anyhow!(
                                    "呼叫 CreateCoreWebView2Controller 失敗: {e}"
                                )));
                            }
                        }
                    });
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }

                Ok(())
            },
        ));

        let create_res = unsafe {
            CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                PCWSTR(user_data_wide.as_ptr()),
                None::<&ICoreWebView2EnvironmentOptions>,
                &env_handler,
            )
        };
        if let Err(e) = create_res {
            STA_CTX.with(|cell| {
                if let Some(ctx) = cell.borrow_mut().as_mut() {
                    if let Some(tx) = ctx.init_tx.take() {
                        let _ = tx.send(Err(anyhow!(
                            "CreateCoreWebView2EnvironmentWithOptions 失敗: {e}"
                        )));
                    }
                }
            });
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }

        let mut msg = MSG::default();
        unsafe {
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            CoUninitialize();
        }
    }

    struct WebViewSession {
        handle: WebViewHandle,
    }

    impl WebViewSession {
        async fn launch(user_data_dir: &Path) -> Result<Self> {
            std::fs::create_dir_all(user_data_dir)?;
            let dir_str = user_data_dir.to_string_lossy().to_string();
            let (init_tx, init_rx) = oneshot::channel();

            std::thread::Builder::new()
                .name("ycm-webview2-sta".to_string())
                .spawn(move || run_sta_webview_thread(dir_str, init_tx))
                .map_err(|e| anyhow!("建立 WebView2 STA 執行緒失敗: {e}"))?;

            let handle = tokio::time::timeout(Duration::from_secs(20), init_rx)
                .await
                .map_err(|_| anyhow!("等待 Edge WebView2 初始化逾時（20 秒）"))??
                ?;

            Ok(Self { handle })
        }

        fn wake_sta(&self) {
            let hwnd = HWND(self.handle.hwnd_raw as *mut c_void);
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_APP_CMD, WPARAM(0), LPARAM(0));
            }
        }

        async fn navigate(&self, url: &str) -> Result<()> {
            let (tx, rx) = oneshot::channel();
            self.handle
                .cmd_tx
                .send(WebViewCmd::Navigate {
                    url: url.to_string(),
                    reply: tx,
                })
                .map_err(|_| anyhow!("WebView2 指令通道已關閉"))?;
            self.wake_sta();

            tokio::time::timeout(Duration::from_secs(30), rx)
                .await
                .map_err(|_| anyhow!("等待網頁載入 ({url}) 逾時"))??
        }

        async fn eval_js(&self, script: &str) -> Result<Value> {
            let (tx, rx) = oneshot::channel();
            self.handle
                .cmd_tx
                .send(WebViewCmd::EvalJs {
                    script: script.to_string(),
                    reply: tx,
                })
                .map_err(|_| anyhow!("WebView2 指令通道已關閉"))?;
            self.wake_sta();

            tokio::time::timeout(Duration::from_secs(15), rx)
                .await
                .map_err(|_| anyhow!("執行頁面 JS 逾時"))??
        }

        async fn capture_png(&self, file_path: &Path) -> Result<String> {
            let (tx, rx) = oneshot::channel();
            self.handle
                .cmd_tx
                .send(WebViewCmd::CapturePng {
                    file_path: file_path.to_string_lossy().to_string(),
                    reply: tx,
                })
                .map_err(|_| anyhow!("WebView2 指令通道已關閉"))?;
            self.wake_sta();

            tokio::time::timeout(Duration::from_secs(15), rx)
                .await
                .map_err(|_| anyhow!("WebView2 截圖逾時"))??
        }

        fn close(&self) {
            let _ = self.handle.cmd_tx.send(WebViewCmd::Close);
            self.wake_sta();
        }
    }

    pub async fn execute_webview2(
        req: &ExecutionRequest,
        tx: &UnboundedSender<RunnerEvent>,
        total_ops: usize,
    ) -> Result<()> {
        emit_status(
            tx,
            "啟動瀏覽器",
            "啟動內嵌 Microsoft Edge WebView2（無須開啟遠端除錯）",
        );

        let profile_dir = std::env::var_os("LOCALAPPDATA")
            .map(|p| Path::new(&p).join("ycm_batch_scheduler").join("webview2_profile"))
            .unwrap_or_else(|| std::env::temp_dir().join("ycm_webview2_profile"));

        emit_log(
            tx,
            "INFO",
            format!(
                "以原生 Edge WebView2 COM 介面啟動視窗（免遠端除錯埠），設定檔目錄：{}",
                profile_dir.display()
            ),
        );

        let session = WebViewSession::launch(&profile_dir).await?;
        let run_result = run_webview_workflow(&session, req, tx, total_ops).await;

        match run_result {
            Ok(completed) => {
                session.close();
                emit_status(tx, "已完成", format!("全數執行完成（共 {completed} 筆）"));
                append_file_log("DONE", &format!("全數執行完成（共 {completed} 筆）"));
                let _ = tx.send(RunnerEvent::Completed {
                    total_executed: completed,
                });
                Ok(())
            }
            Err(err) => {
                emit_log(
                    tx,
                    "WARN",
                    format!("執行異常：{err}（保留 WebView2 視窗 10 秒供檢視頁面狀態）"),
                );
                emit_status(tx, "執行異常", "保留 WebView2 視窗 10 秒供檢視原因...");
                tokio::time::sleep(Duration::from_secs(10)).await;
                session.close();
                Err(err)
            }
        }
    }

    async fn run_webview_workflow(
        wv: &WebViewSession,
        req: &ExecutionRequest,
        tx: &UnboundedSender<RunnerEvent>,
        total_ops: usize,
    ) -> Result<usize> {
        emit_status(tx, "開啟網址", format!("導航至 {}", req.config.target_url));
        emit_log(tx, "INFO", format!("導航至目標網址：{}", req.config.target_url));
        wv.navigate(&req.config.target_url).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;

        if let Ok(info) = wv
            .eval_js("JSON.stringify({ href: location.href, title: document.title, readyState: document.readyState })")
            .await
        {
            if let Some(s) = info.as_str() {
                emit_log(tx, "INFO", format!("頁面載入狀態：{s}"));
            }
        }

        let timeout = Duration::from_secs(req.config.action_timeout_sec.max(15));

        if !req.config.pin.trim().is_empty() {
            emit_status(tx, "登入檢查", "檢查是否需要輸入 PIN 碼登入 YCM");
            if wait_for_selector(wv, "#pin", Duration::from_secs(6)).await.is_ok() {
                set_input_value(wv, "#pin", &req.config.pin).await?;
                click_selector(wv, "button.login-btn").await?;
                emit_log(tx, "INFO", "PIN 碼送出登入完成，等待主畫面載入...");
                tokio::time::sleep(Duration::from_millis(800)).await;
            }
        }

        emit_status(
            tx,
            "等待畫面就緒",
            "等待 #SampleGrid 載入（若需憑證或手動登入，請直接於彈出的 WebView2 視窗中操作）",
        );

        let mut completed = 0usize;
        let mut is_very_first_navigation = true;

        if req.group.group_mode.eq_ignore_ascii_case("SINGLE") {
            let step = &req.steps[0];
            let mut is_first_batch = true;

            for sched in &req.schedule_dates {
                let base_date = RocDateUtils::parse_roc_date(&sched.roc_date_raw)
                    .unwrap_or_else(|| Local::now().date_naive());
                let resolver = ParamResolver::new(base_date).with_custom_vars(req.custom_vars.clone());

                if is_first_batch {
                    open_job_form(
                        wv,
                        &req.config.target_url,
                        &step.program.job_code,
                        timeout,
                        is_very_first_navigation,
                    )
                    .await?;
                    is_very_first_navigation = false;
                    is_first_batch = false;
                } else {
                    emit_status(tx, "等待F6啟用", "輪詢 #btnF6 (F6修改再執行) 變為可用狀態");
                    wait_enabled(wv, "#btnF6", timeout).await?;
                    click_selector(wv, "#btnF6").await?;
                    wait_enabled(wv, "#btnF8", timeout).await?;
                }

                emit_status(
                    tx,
                    "填寫參數",
                    format!("填寫 {} ({})", step.program.job_code, sched.roc_date),
                );
                fill_step_fields(wv, &resolver, step, sched, None).await?;

                emit_status(tx, "F8送出", format!("送出 {} 排程", step.program.job_code));
                click_selector(wv, "#btnF8").await?;
                let sn = wait_op_msg_sn(wv, timeout).await?;
                let shot_path = save_step_screenshot(
                    wv,
                    &req.config.screenshot_dir,
                    &step.program.job_code,
                    &sched.roc_date_raw,
                    completed + 1,
                )
                .await?;

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
                    open_job_form(
                        wv,
                        &req.config.target_url,
                        &step.program.job_code,
                        timeout,
                        is_very_first_navigation,
                    )
                    .await?;
                    is_very_first_navigation = false;

                    emit_status(
                        tx,
                        "填寫參數",
                        format!("填寫 {} ({})", step.program.job_code, sched.roc_date),
                    );
                    fill_step_fields(wv, &resolver, step, sched, prev_sn.as_deref()).await?;

                    emit_status(tx, "F8送出", format!("送出 {} 排程", step.program.job_code));
                    click_selector(wv, "#btnF8").await?;
                    let sn = wait_op_msg_sn(wv, timeout).await?;
                    prev_sn = Some(sn.clone());

                    let shot_path = save_step_screenshot(
                        wv,
                        &req.config.screenshot_dir,
                        &step.program.job_code,
                        &sched.roc_date_raw,
                        completed + 1,
                    )
                    .await?;

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

    async fn open_job_form(
        wv: &WebViewSession,
        target_url: &str,
        job_code: &str,
        timeout: Duration,
        skip_initial_goto: bool,
    ) -> Result<()> {
        if !skip_initial_goto {
            wv.navigate(target_url).await?;
        }
        let grid_timeout = if skip_initial_goto {
            timeout.max(Duration::from_secs(60))
        } else {
            timeout
        };
        wait_sample_grid_ready(wv, grid_timeout).await?;
        set_input_value(wv, "#fuzzyInquiry", job_code).await?;
        click_selector(wv, "#btnF4").await?;
        tokio::time::sleep(Duration::from_millis(350)).await;
        wait_sample_grid_ready(wv, timeout).await?;

        let clicked = wv
            .eval_js(
                "(() => { const rows = document.querySelectorAll('#SampleGrid tr'); if (rows.length < 2) return false; rows[1].click(); return true; })()",
            )
            .await?
            .as_bool()
            .unwrap_or(false);

        if !clicked {
            return Err(anyhow!("搜尋 [{job_code}] 後 #SampleGrid 無可選資料列"));
        }

        tokio::time::sleep(Duration::from_millis(300)).await;
        click_selector(wv, "#btnF1").await?;

        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(val) = wv
                .eval_js("(document.querySelector('#jobCodeNumber')?.value || '').trim()")
                .await
            {
                if val.as_str().unwrap_or_default() == job_code {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }

        Err(anyhow!("驗證 #jobCodeNumber == {job_code} 超時"))
    }

    async fn fill_step_fields(
        wv: &WebViewSession,
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
            let selector = format!("#{}", param.html_id);
            set_input_value(wv, &selector, &val).await?;
        }

        set_checkbox(wv, "#reqOPConfirm", step.item.req_op_confirm).await?;
        set_checkbox(wv, "#reqEmail", step.item.req_email).await?;

        if step.item.req_email && !step.item.mail_recipients.trim().is_empty() {
            let resolved_mail = resolver.resolve_template(&step.item.mail_recipients);
            set_input_value(wv, "#mailRecipients", &resolved_mail).await?;
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
            set_input_value(wv, "#preJobSn1", &pre1).await?;
        }
        if !step.item.pre_job_sn2.is_empty() {
            set_input_value(wv, "#preJobSn2", &step.item.pre_job_sn2).await?;
        }
        if !step.item.pre_job_sn3.is_empty() {
            set_input_value(wv, "#preJobSn3", &step.item.pre_job_sn3).await?;
        }
        if !step.item.rework_sn.is_empty() {
            set_input_value(wv, "#reWorkSn", &step.item.rework_sn).await?;
        }
        if !step.item.pre_work_note.is_empty() {
            set_input_value(wv, "#preWork", &step.item.pre_work_note).await?;
        }
        if !step.item.rear_work_note.is_empty() {
            set_input_value(wv, "#rearWork", &step.item.rear_work_note).await?;
        }

        let exe_date = resolve_step_date(resolver, &step.item.expect_exe_date_rule, sched);
        set_input_value(wv, "#expectExeDate", &exe_date).await?;

        let exe_time = if step.item.expect_exe_time.trim().is_empty() {
            sched.exe_time.clone()
        } else {
            step.item.expect_exe_time.clone()
        };
        if !exe_time.is_empty() {
            set_input_value(wv, "#expectExeTime", &exe_time).await?;
        }

        Ok(())
    }

    async fn wait_for_selector(wv: &WebViewSession, selector: &str, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        let script = format!("Boolean(document.querySelector({:?}))", selector);
        while Instant::now() < deadline {
            if let Ok(res) = wv.eval_js(&script).await {
                if res.as_bool().unwrap_or(false) {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Err(anyhow!("等待元素 {selector} 超時"))
    }

    async fn wait_sample_grid_ready(wv: &WebViewSession, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(res) = wv
                .eval_js("document.querySelectorAll('#SampleGrid tr').length >= 2")
                .await
            {
                if res.as_bool().unwrap_or(false) {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
        Err(anyhow!("等待 #SampleGrid 載入超時"))
    }

    async fn wait_enabled(wv: &WebViewSession, selector: &str, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        let script = format!(
            "(() => {{ const el = document.querySelector({:?}); return Boolean(el && !el.disabled); }})()",
            selector
        );
        while Instant::now() < deadline {
            if let Ok(res) = wv.eval_js(&script).await {
                if res.as_bool().unwrap_or(false) {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        Err(anyhow!("等待元素 {selector} 變為可用狀態超時"))
    }

    async fn set_input_value(wv: &WebViewSession, selector: &str, value: &str) -> Result<()> {
        let script = format!(
            r#"(() => {{
                const el = document.querySelector({:?});
                if (!el) return false;
                el.focus();
                el.value = {:?};
                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return true;
            }})()"#,
            selector, value
        );
        let ok = wv.eval_js(&script).await?.as_bool().unwrap_or(false);
        if !ok {
            return Err(anyhow!("找不到表單欄位 {selector}"));
        }
        Ok(())
    }

    async fn set_checkbox(wv: &WebViewSession, selector: &str, checked: bool) -> Result<()> {
        let script = format!(
            r#"(() => {{
                const el = document.querySelector({:?});
                if (!el) return false;
                if (Boolean(el.checked) !== {:?}) {{
                    el.click();
                }}
                return true;
            }})()"#,
            selector, checked
        );
        let _ = wv.eval_js(&script).await?;
        Ok(())
    }

    async fn click_selector(wv: &WebViewSession, selector: &str) -> Result<()> {
        let script = format!(
            r#"(() => {{
                const el = document.querySelector({:?});
                if (!el || el.disabled) return false;
                el.click();
                return true;
            }})()"#,
            selector
        );
        let ok = wv.eval_js(&script).await?.as_bool().unwrap_or(false);
        if !ok {
            return Err(anyhow!("無法點擊元素 {selector} (不存在或為 Disabled)"));
        }
        Ok(())
    }

    async fn wait_op_msg_sn(wv: &WebViewSession, timeout: Duration) -> Result<String> {
        let re = Regex::new(r"\d+")?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(res) = wv
                .eval_js("(document.querySelector('#OpMsg')?.innerText || '').trim()")
                .await
            {
                let text = res.as_str().unwrap_or_default();
                if text.contains("新增成功") || text.contains("成功") {
                    if let Some(m) = re.find(text) {
                        return Ok(m.as_str().to_string());
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err(anyhow!("等待 #OpMsg 取得工作序號超時"))
    }

    async fn save_step_screenshot(
        wv: &WebViewSession,
        dir: &str,
        job_code: &str,
        roc_date_raw: &str,
        seq: usize,
    ) -> Result<String> {
        std::fs::create_dir_all(dir)?;
        let filename = format!("{}_{}_{:03}.png", job_code, roc_date_raw, seq);
        let path = Path::new(dir).join(filename);
        wv.capture_png(&path).await
    }
}

#[cfg(windows)]
pub use win_impl::execute_webview2;

#[cfg(not(windows))]
pub async fn execute_webview2(
    _req: &crate::engine::browser_runner::ExecutionRequest,
    _tx: &tokio::sync::mpsc::UnboundedSender<crate::engine::browser_runner::RunnerEvent>,
    _total_ops: usize,
) -> anyhow::Result<()> {
    Err(anyhow::anyhow!(
        "Microsoft Edge WebView2 自動化僅支援 Windows 桌面環境；目前作業系統請使用預演模式 (Dry-Run)"
    ))
}
