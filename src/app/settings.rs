//! 系統設定與自訂變數：動作與畫面

use super::*;
use super::helpers::{labeled_input};

impl BatchSchedulerApp {
    // --- Actions: Settings & Variables ---
    pub(super) fn save_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let edge_path = self.cfg_edge_path_input.read(cx).value().trim().to_string();
        let target_url = self.cfg_target_url_input.read(cx).value().trim().to_string();
        let pin = self.cfg_pin_input.read(cx).value().trim().to_string();
        let screenshot_dir = self
            .cfg_screenshot_dir_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let timeout_sec = self.cfg_timeout_input.read(cx).value().trim().to_string();

        let guard = self.conn.lock().unwrap();
        let repo = Repository::new(&guard);
        let pairs = [
            ("edge_path", edge_path),
            ("target_url", target_url),
            ("default_pin", pin),
            ("screenshot_dir", screenshot_dir),
            ("action_timeout_sec", timeout_sec),
        ];
        for (k, v) in pairs {
            if let Err(e) = repo.set_config(k, &v) {
                window.push_notification(
                    Notification::error(format!("儲存設定 {k} 失敗：{e}")),
                    cx,
                );
                return;
            }
        }
        window.push_notification(Notification::success("系統全域設定已成功儲存！"), cx);
    }

    pub(super) fn render_settings_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .items_start()
            .gap_4()
            .child(
                v_flex()
                    .w_full()
                    .items_start()
                    .gap_1()
                    .child(div().text_xl().font_weight(FontWeight::BOLD).child("設定"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(MUTED_FOREGROUND)
                            .child("系統全域設定"),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(840.))
                    .items_start()
                    .bg(CARD)
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(BORDER)
                    .gap_4()
                    .child(labeled_input(
                        "Edge 瀏覽器執行檔路徑 (edge_path，UIA 模式使用)",
                        &self.cfg_edge_path_input,
                    ))
                    .child(labeled_input("YCM 目標網址 (target_url)", &self.cfg_target_url_input))
                    .child(labeled_input("預設 PIN 碼 (default_pin)", &self.cfg_pin_input))
                    .child(labeled_input("截圖存檔目錄 (screenshot_dir)", &self.cfg_screenshot_dir_input))
                    .child(labeled_input("元素等待逾時秒數 (action_timeout_sec)", &self.cfg_timeout_input))
                    .child(
                        div().child(
                            Button::new("btn-save-settings")
                                .primary()
                                .label("儲存全域設定")
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    this.save_settings(window, cx);
                                })),
                        ),
                    ),
            )
    }
}
