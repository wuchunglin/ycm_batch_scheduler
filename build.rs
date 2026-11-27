//! 編譯時把 app.ico 嵌入 Windows 執行檔，讓 exe 檔案、工作列與 Alt+Tab 都顯示圖示

fn main() {
    println!("cargo:rerun-if-changed=app.ico");
    println!("cargo:rerun-if-changed=build.rs");

    // 以「目標平台」判斷（不是編譯主機），非 Windows 目標直接略過
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        #[cfg(windows)]
        {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("app.ico");
            res.set("ProductName", "YCM 共用批次排程自動化執行系統");
            res.set("FileDescription", "YCM 共用批次排程自動化執行系統");
            if let Err(e) = res.compile() {
                // 沒有圖示不影響程式功能，只提示不讓整個編譯失敗
                println!("cargo:warning=嵌入圖示失敗：{e}");
            }
        }
    }
}
