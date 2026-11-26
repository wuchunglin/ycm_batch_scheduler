//! 自動化執行：啟動執行、參數解析與監控台畫面

use super::*;

impl BatchSchedulerApp {
    // --- Actions: Execution Runner ---
    pub(super) fn start_execution(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.runner_state.is_running {
            return;
        }
        let Some(group) = self.selected_group.clone() else {
            window.push_notification(Notification::error("請先於「排程群組設定」建立並選取群組"), cx);
            return;
        };

        let (steps, custom_vars) = {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let items = repo.list_group_items(group.id.unwrap_or(0)).unwrap_or_default();
            let mut steps = Vec::new();
            for item in items {
                if let Ok(Some(program)) = repo.get_program_by_id(item.program_id) {
                    let params = repo
                        .list_item_params_detailed(item.id.unwrap_or(0), item.program_id)
                        .unwrap_or_default();
                    steps.push(ExecutionStepInput {
                        item,
                        program,
                        params,
                    });
                }
            }
            let vars = repo.get_variables_map().unwrap_or_default();
            (steps, vars)
        };

        if steps.is_empty() {
            window.push_notification(
                Notification::error("群組內尚無任何程式步驟，請先加入批次程式"),
                cx,
            );
            return;
        }

        let override_date = self
            .runner_override_date_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let schedule_dates = if !override_date.is_empty() {
            if let Some(d) = RocDateUtils::parse_roc_date(&override_date) {
                vec![GeneratedScheduleItem {
                    roc_date: RocDateUtils::to_roc_yyymmdd(&d, "/"),
                    roc_date_raw: RocDateUtils::to_roc_yyymmdd(&d, ""),
                    exe_time: self.schedule_rule.default_time.clone(),
                }]
            } else {
                window.push_notification(
                    Notification::error("單日測試日期格式錯誤，請使用 YYY/MM/DD（如 115/11/24）"),
                    cx,
                );
                return;
            }
        } else if self.preview_items.is_empty() {
            let today = Local::now().date_naive();
            vec![GeneratedScheduleItem {
                roc_date: RocDateUtils::to_roc_yyymmdd(&today, "/"),
                roc_date_raw: RocDateUtils::to_roc_yyymmdd(&today, ""),
                exe_time: self.schedule_rule.default_time.clone(),
            }]
        } else {
            self.preview_items.clone()
        };

        let config = self.build_runner_config(cx);
        let req = ExecutionRequest {
            config,
            group,
            steps,
            schedule_dates,
            custom_vars,
        };
        self.launch_execution(req, cx);
    }

    /// 由「設定」頁的輸入框組出瀏覽器執行設定
    pub(super) fn build_runner_config(&self, cx: &Context<Self>) -> BrowserRunnerConfig {
        BrowserRunnerConfig {
            edge_path: self.cfg_edge_path_input.read(cx).value().trim().to_string(),
            target_url: self.cfg_target_url_input.read(cx).value().trim().to_string(),
            pin: self.cfg_pin_input.read(cx).value().trim().to_string(),
            screenshot_dir: self
                .cfg_screenshot_dir_input
                .read(cx)
                .value()
                .trim()
                .to_string(),
            action_timeout_sec: self
                .cfg_timeout_input
                .read(cx)
                .value()
                .trim()
                .parse()
                .unwrap_or(30),
            dry_run: self.runner_dry_run,
        }
    }

    /// 啟動背景執行並持續把事件（日誌、進度）接回畫面；日誌同時寫入 logs/ycm_run_日期.log
    pub(super) fn launch_execution(&mut self, req: ExecutionRequest, cx: &mut Context<Self>) {
        let steps_per_date = req.steps.len();
        let total = req.schedule_dates.len() * steps_per_date;
        self.runner_state.is_running = true;
        self.runner_state.failed = false;
        self.runner_state.captured_sns.clear();
        self.runner_state.reset_logs();
        self.runner_state.steps_per_date = steps_per_date;
        self.runner_state.progress = (0, total);
        self.runner_state.current_step = "啟動中...".to_string();
        cx.notify();

        let (tx, mut rx) = mpsc::unbounded_channel::<RunnerEvent>();
        self.tokio_rt.spawn(async move {
            execute_group(req, tx).await;
        });

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;

                let mut drained = Vec::new();
                let mut disconnected = false;
                loop {
                    match rx.try_recv() {
                        Ok(ev) => drained.push(ev),
                        Err(mpsc::error::TryRecvError::Empty) => break,
                        Err(mpsc::error::TryRecvError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                }

                if !drained.is_empty() {
                    let mut still_running = true;
                    let updated = this.update(cx, |this, cx| {
                        for ev in drained {
                            this.runner_state.handle_event(ev);
                        }
                        still_running = this.runner_state.is_running;
                        cx.notify();
                    });
                    if updated.is_err() || !still_running || disconnected {
                        break;
                    }
                } else if disconnected {
                    let _ = this.update(cx, |this, cx| {
                        this.runner_state.is_running = false;
                        cx.notify();
                    });
                    break;
                }
            }
        })
        .detach();
    }


    pub(super) fn build_resolver(&self, cx: &Context<Self>) -> ParamResolver {
        let date_str = self.tester_date_input.read(cx).value().to_string();
        let base_date = RocDateUtils::parse_roc_date(&date_str)
            .unwrap_or_else(|| Local::now().date_naive());
        let vars: HashMap<String, String> = self
            .custom_variables
            .iter()
            .map(|v| (v.var_key.clone(), v.var_value.clone()))
            .collect();
        ParamResolver::new(base_date).with_custom_vars(vars)
    }

    pub(super) fn render_runner_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (curr, total) = self.runner_state.progress;
        let ratio = if total > 0 {
            (curr as f32 / total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        v_flex()
            .w_full()
            .items_start()
            .gap_5()
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .justify_between()
                    .items_center()
                    .gap_3()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child("自動化執行監控台"),
                            )
                            .child(div().text_sm().text_color(MUTED_FOREGROUND).child(format!(
                                "目前群組：{} | 雙軌狀態機 (Windows UI Automation 操作桌面 Edge)",
                                self.selected_group
                                    .as_ref()
                                    .map(|g| g.group_name.as_str())
                                    .unwrap_or("未選取")
                            ))),
                    )
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_3()
                            .items_center()
                            .child(
                                Checkbox::new("chk-dry-run")
                                    .label("預演模式")
                                    .checked(self.runner_dry_run)
                                    .on_click(cx.listener(|this, chk, _, cx| {
                                        this.runner_dry_run = *chk;
                                        cx.notify();
                                    })),
                            )
                            .child(div().w(px(230.)).min_w(px(160.)).child(Input::new(&self.runner_override_date_input)))
                            .child(
                                Button::new("btn-start-run")
                                    .primary()
                                    .label(if self.runner_state.is_running {
                                        "執行中..."
                                    } else {
                                        "開始執行排程作業"
                                    })
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.start_execution(window, cx);
                                    })),
                            ),
                    ),
            )
            // Progress Bar
            .child(
                v_flex()
                    .w_full()
                    .bg(CARD)
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(BORDER)
                    .gap_3()
                    .child(
                        h_flex()
                            .w_full()
                            .flex_wrap()
                            .justify_between()
                            .items_center()
                            .gap_2()
                            .child(
                                h_flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .items_center()
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child("執行進度"))
                                    .child(Tag::secondary().child(self.runner_state.current_step.clone())),
                            )
                            .child(div().text_xs().child(format!("{curr}/{total} 完成"))),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(10.))
                            .bg(BACKGROUND)
                            .rounded_full()
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui::relative(if curr > 0 { ratio.max(0.02) } else { 0.0 }))
                                    .bg(SUCCESS)
                                    .rounded_full(),
                            ),
                    ),
            )
            // Captured SN Badges
            .child(if self.runner_state.captured_sns.is_empty() {
                div().into_any_element()
            } else {
                let mut row = h_flex().w_full().gap_2().flex_wrap().p_3().bg(CARD).rounded_lg().border_1().border_color(BORDER);
                for item in &self.runner_state.captured_sns {
                    row = row.child(Tag::success().child(format!("{} #{}", item.job_code, item.sn)));
                }
                row.into_any_element()
            })
            // Log Stream
            .child(
                v_flex()
                    .w_full()
                    .flex_1()
                    .bg(BACKGROUND)
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(BORDER)
                    .gap_2()
                    .child(
                        h_flex()
                            .w_full()
                            .flex_wrap()
                            .justify_between()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(MUTED_FOREGROUND)
                                    .child("即時執行日誌串流 (Real-time Log Stream)"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(MUTED_FOREGROUND)
                                    .child(format!("實體日誌檔：{}", current_log_file_path())),
                            ),
                    )
                    .child({
                        let mut logs = v_flex().gap_1();
                        for log in &self.runner_state.logs {
                            let color = if log.contains("ERROR") || log.contains("FAILED") {
                                DANGER
                            } else if log.contains("SUCCESS") || log.contains("DONE") {
                                SUCCESS
                            } else if log.contains("SCREENSHOT") || log.contains("WARN") {
                                WARNING
                            } else {
                                FOREGROUND
                            };
                            logs = logs.child(
                                div()
                                    .text_xs()
                                    .font_family(SharedString::from(
                                        "Consolas, Courier New, monospace",
                                    ))
                                    .text_color(color)
                                    .child(log.clone()),
                            );
                        }
                        logs
                    }),
            )
    }
}
