#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod theme;

use std::sync::{Arc, Mutex};
use gpui::{
    px, size, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions,
};
use gpui_component::Root;
use gpui_component_assets::Assets;
use rusqlite::Connection;

use app::BatchSchedulerApp;
use ycm_batch_scheduler::db::schema::init_db;

fn main() -> anyhow::Result<()> {
    let db_path = "ycm_scheduler.db";
    let conn = Connection::open(db_path)?;
    init_db(&conn)?;

    let shared_conn = Arc::new(Mutex::new(conn));
    let tokio_rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?,
    );

    let application = Application::new().with_assets(Assets);

    application.run(move |cx| {
        gpui_component::init(cx);
        theme::apply(cx);

        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1280.), px(840.)),
                cx,
            ))),
            window_min_size: Some(size(px(480.), px(480.))),
            titlebar: Some(TitlebarOptions {
                title: Some("YCM 共用批次排程自動化執行系統 (GPUI 通用版)".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let conn_for_window = shared_conn.clone();
        let rt_for_window = tokio_rt.clone();
        cx.spawn(async move |cx| {
            cx.open_window(options, |window, cx| {
                let view =
                    cx.new(|cx| BatchSchedulerApp::new(conn_for_window, rt_for_window, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })?;
            Ok::<_, anyhow::Error>(())
        })
        .detach();

        cx.activate(true);
    });

    Ok(())
}
