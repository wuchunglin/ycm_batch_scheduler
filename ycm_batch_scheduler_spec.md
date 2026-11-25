# YCM 共用批次排程自動化執行系統 — 細部設計規格書（GPUI 版）

## 1. 系統概述與技術架構

### 1.1 專案目標
打造一套可編譯為**單一獨立執行檔（`.exe`）**的跨作業排程自動化桌面應用程式，用於在財政部「資源管理-操管自動化管理系統（YCM）」的「不定期作業排程（YCM230W）」中，管理批次程式參數、組合排程群組，並透過瀏覽器底層協定自動完成排程設定與截圖存證[cite: 1, 2]。

### 1.2 核心技術選型（全靜態單檔 + GPU 加速原生 GUI）
為適應內網無外網環境、免除外部執行期依賴（無需 Node.js 或 WebView2），全專案採用純 Rust 生態系建置：
* **開發語言**：Rust (Edition 2021)
* **桌面 GUI 框架**：**`gpui`**（Zed 團隊開發之 GPU 加速原生 Rust UI 框架，Windows 採 DirectX / Blade 渲染）＋ **`gpui-component`**（提供 Input、Select 下拉選單、Checkbox、Radio、Table 等標準表單元件）
* **瀏覽器自動化引擎**：`chromiumoxide`（直接透過 CDP 協定驅動本機已安裝之 `msedge.exe`）[cite: 4]
* **本機資料庫**：`rusqlite`（開啟 `features = ["bundled"]`，將 SQLite C 原始碼靜態編譯進執行檔）
* **HTML 解析器**：`scraper`（用於解析 YCM `YCM321-3W2` 表單頁面結構）[cite: 2]
* **非同步執行期**：`tokio`（背景執行瀏覽器自動化，透過 `tokio::sync::mpsc` 與 GPUI 主執行緒通訊更新畫面狀態）
* **日期與時間運算**：`chrono`（處理西元與中華民國紀年轉換、大小月與閏年計算）

---

## 2. GPUI 桌面介面與功能模組規劃

系統採左側導航欄（Sidebar）＋ 右側工作區（Workspace）的雙欄桌面視窗佈局，劃分為五大核心視圖模組：

1. **批次程式與欄位管理（`ProgramManagerView`）**：
   * 顯示已登錄的批次程式清單（`programs`）。
   * 提供「匯入 HTML 解析」按鈕（開啟檔案選擇器讀取 `html.txt`）與「人工新增/編輯欄位」表單[cite: 2]。
2. **排程群組與參數設定（`GroupEditorView`）**：
   * 建立或編輯排程群組，選擇群組執行模式（`SINGLE` 單一程式多批次 / `MULTI` 多程式串接）。
   * 設定群組內各支程式的**「共同項目」**（預計執行時間、OP 確認、前置序號 1~3、重作序號、Mail 通知與收件者、前後置備註）[cite: 2]。
   * 設定各程式**「專屬參數」**，提供**圖 1 的 `E1`~`EG` 動態代碼下拉選單**及擴充樣板設定[cite: 6]。
3. **排程時間生成器（`ScheduleRuleView`）**：
   * 完整實作**圖 2 的排程時間定義控制項**（年度別複選、整年/時間區間單選、每月/每週/指定日期三選一模式、預計執行時間）[cite: 7]。
   * 右側面板即時渲染「展開後排程清單預覽表格」，列出每筆預計執行日期與換算後的真實參數值。
4. **自動化執行監控台（`ExecutionRunnerView`）**：
   * 選擇群組並點擊「開始執行排程」後，即時顯示執行進度條、即時日誌（Log Stream）、目前狀態（`登入中`、`填寫參數`、`F8送出`、`等待F6啟用`、`截圖存證`）[cite: 1, 2]，以及擷取到的工作序號清單。
5. **系統全域設定（`SettingsView`）**：
   * 管理本機 `msedge.exe` 路徑、YCM 目標網址、預設 PIN 碼與截圖存檔資料夾[cite: 1, 4]。

---

## 3. SQLite 資料庫 Schema 設計

資料庫檔案預設存放於執行檔同目錄下（`ycm_scheduler.db`），包含以下 6 張核心資料表：

### 3.1 `sys_config`（系統全域設定表）
| 欄位名稱 | 型別 | 說明 | 預設值範例 |
| :--- | :--- | :--- | :--- |
| `key` | TEXT (PK) | 設定鍵名 | `edge_path`, `target_url`, `default_pin`, `screenshot_dir` |
| `value` | TEXT | 設定值 | `https://www.nat.fia.gov.tw/ycm/jobExecute/unRegular/getSysData`[cite: 1] |

### 3.2 `programs`（批次程式主檔）
| 欄位名稱 | 型別 | 說明 | 範例 |
| :--- | :--- | :--- | :--- |
| `id` | INTEGER (PK) | 自動遞增主鍵 | `1` |
| `job_code` | TEXT (UNIQUE) | 工作/程式代號[cite: 1, 2] | `DAV140XJ`[cite: 1, 2] |
| `job_name` | TEXT | 工作名稱[cite: 2] | `統計遺產稅課稅資料索引檔－J`[cite: 2] |
| `created_at` | DATETIME | 建立時間 | `2026-11-24 10:00:00` |

### 3.3 `program_fields`（程式專屬輸入欄位定義表）
| 欄位名稱 | 型別 | 說明 | 範例 |
| :--- | :--- | :--- | :--- |
| `id` | INTEGER (PK) | 自動遞增主鍵 | `1` |
| `program_id` | INTEGER (FK) | 關聯 `programs.id` | `1` |
| `field_scope` | TEXT | 所屬區塊：`JOB`（`#jobParaTb`）或 `PROGRAM`（`#programSet`）[cite: 1, 2] | `PROGRAM` |
| `html_id` | TEXT | 網頁元素 ID[cite: 2] | `programParameter_0_0`[cite: 2] |
| `html_name` | TEXT | 網頁元素 Name[cite: 2] | `programParameter_0_0`[cite: 2] |
| `label` | TEXT | 畫面顯示的中文提示名稱[cite: 2] | `請輸入轉檔起始日期(yyymmdd)`[cite: 2] |
| `tag_type` | TEXT | 元素類型（`input`, `select`, `textarea`） | `input`[cite: 2] |
| `max_length` | INTEGER | 最大長度限制（0 表無限制）[cite: 2] | `7`[cite: 2] |
| `is_required` | BOOLEAN | 是否為必填欄位[cite: 2] | `1` |
| `sort_order` | INTEGER | 欄位填寫順序 | `0` |

### 3.4 `job_groups`（排程群組主檔）
| 欄位名稱 | 型別 | 說明 | 範例 |
| :--- | :--- | :--- | :--- |
| `id` | INTEGER (PK) | 自動遞增主鍵 | `1` |
| `group_name` | TEXT (UNIQUE) | 群組名稱 | `DAV死亡資料每週灌檔排程` |
| `description` | TEXT | 群組說明 | `每週五 11:50 與 13:00 執行之 9 支死亡資料批次`[cite: 1] |
| `group_mode` | TEXT | 執行模式：`SINGLE`（單一程式多批次）或 `MULTI`（多程式串接） | `MULTI` |
| `schedule_rule_json` | TEXT | 儲存圖 2 排程時間定義的 JSON 結構[cite: 7] | 見第 5 節說明 |
| `created_at` | DATETIME | 建立時間 | `2026-11-24 10:30:00` |

### 3.5 `group_items`（群組程式明細與共同項目設定表）
| 欄位名稱 | 型別 | 說明 | 範例 |
| :--- | :--- | :--- | :--- |
| `id` | INTEGER (PK) | 自動遞增主鍵 | `1` |
| `group_id` | INTEGER (FK) | 關聯 `job_groups.id` | `1` |
| `program_id` | INTEGER (FK) | 關聯 `programs.id` | `1` |
| `step_order` | INTEGER | 執行順序（1, 2, 3...） | `1` |
| `expect_exe_date_rule` | TEXT | 預計執行日期規則（預設採用群組展開日，或指定如 `WEEK_FRI`）[cite: 1] | `GROUP_DATE` |
| `expect_exe_time` | TEXT | 預計執行時間（`HH:mm`，空白表當下系統時間）[cite: 2] | `13:00`[cite: 1] |
| `req_op_confirm` | BOOLEAN | 是否 OP 確認（`#reqOPConfirm`）[cite: 2] | `0` |
| `link_prev_sn` | BOOLEAN | 是否自動帶入上一步驟產生的工作序號至 `#preJobSn1`[cite: 1, 2] | `1` |
| `pre_job_sn1` | TEXT | 靜態指定前置工作序號 1（未勾選自動串接時使用）[cite: 2] | `""` |
| `pre_job_sn2` | TEXT | 前置工作序號 2（`#preJobSn2`）[cite: 2] | `""` |
| `pre_job_sn3` | TEXT | 前置工作序號 3（`#preJobSn3`）[cite: 2] | `""` |
| `rework_sn` | TEXT | 重作工作序號（`#reWorkSn`）[cite: 2] | `""` |
| `req_email` | BOOLEAN | 是否發送 mail 通知（`#reqEmail`）[cite: 2] | `1` |
| `mail_recipients` | TEXT | Mail 收件者（分號分隔，支援動態月份對應規則）[cite: 1, 2] | `ND00442@ntbsa.gov.tw;{MONTH_MAIL}`[cite: 1] |
| `pre_work_note` | TEXT | 前置工作備註說明（`#preWork`）[cite: 2] | `""` |
| `rear_work_note` | TEXT | 後置工作備註說明（`#rearWork`）[cite: 2] | `""` |

### 3.6 `group_item_params`（群組程式專屬參數值設定表）
| 欄位名稱 | 型別 | 說明 | 範例 |
| :--- | :--- | :--- | :--- |
| `id` | INTEGER (PK) | 自動遞增主鍵 | `1` |
| `group_item_id` | INTEGER (FK) | 關聯 `group_items.id` | `1` |
| `field_id` | INTEGER (FK) | 關聯 `program_fields.id` | `1` |
| `rule_code` | TEXT | 參數類型代碼（如 `E1`, `ED`, `CUSTOM_TEMPLATE`, `STATIC`）[cite: 6] | `CUSTOM_TEMPLATE` |
| `rule_value` | TEXT | 規則參數值或樣板字串 | `DAVF222.{E1}.FDC.D01`[cite: 1, 6] |

---

## 4. 動態參數規則引擎（整合圖 1 與擴充選項）

所有日期計算皆以**該筆排程的「預計執行日期（Base Date）」**（若未指定則為系統當日）為基準進行推算，並轉換為中華民國紀年格式（西元年 $- 1911$）[cite: 1, 6]。

### 4.1 完整支援圖 1 原系統代碼（`E1` ~ `EG`）[cite: 6]
| 代碼 | 規則名稱 | 輸出格式 | 計算邏輯與範例（假設執行日為民國 `115/11/24`） |
| :--- | :--- | :--- | :--- |
| **`E1`** | 執行日期當日[cite: 6] | `YYYMMDD` | `BaseDate` → `1151124`[cite: 6] |
| **`E2`** | 執行日期前一天[cite: 6] | `YYYMMDD` | `BaseDate - 1 day` → `1151123`[cite: 6] |
| **`E3`** | 執行日期前二天[cite: 6] | `YYYMMDD` | `BaseDate - 2 days` → `1151122`[cite: 6] |
| **`E4`** | 執行日期前三天[cite: 6] | `YYYMMDD` | `BaseDate - 3 days` → `1151121`[cite: 6] |
| **`E5`** | 執行日期前四天[cite: 6] | `YYYMMDD` | `BaseDate - 4 days` → `1151120`[cite: 6] |
| **`E6`** | 執行日期前五天[cite: 6] | `YYYMMDD` | `BaseDate - 5 days` → `1151119`[cite: 6] |
| **`E7`** | 執行日期前六天[cite: 6] | `YYYMMDD` | `BaseDate - 6 days` → `1151118`[cite: 6] |
| **`E8`** | 執行日期前七天[cite: 6] | `YYYMMDD` | `BaseDate - 7 days` → `1151117`[cite: 6] |
| **`E9`** | 系統/執行日期當月[cite: 6] | `MM` | `BaseDate.month` → `11`[cite: 6] |
| **`EA`** | 前一個月份[cite: 6] | `MM` | `(BaseDate - 1 month).month` → `10`[cite: 6] |
| **`EB`** | 前二個月份[cite: 6] | `MM` | `(BaseDate - 2 months).month` → `09`[cite: 6] |
| **`EC`** | 前三個月分[cite: 6] | `MM` | `(BaseDate - 3 months).month` → `08`[cite: 6] |
| **`ED`** | 系統/執行日期年月[cite: 6] | `YYYMM` | `BaseDate` 年月 → `11511`[cite: 6] |
| **`EE`** | 前一年月[cite: 6] | `YYYMM` | `BaseDate - 1 month` → `11510`[cite: 6] |
| **`EF`** | 前二年月[cite: 6] | `YYYMM` | `BaseDate - 2 months` → `11509`[cite: 6] |
| **`EG`** | 前三年月[cite: 6] | `YYYMM` | `BaseDate - 3 months` → `11508`[cite: 6] |

### 4.2 擴充進階參數選項（滿足實務特殊需求）
1. **天數偏移擴充（`DAY_OFFSET`）**：支援正負天數偏移（例如 `+2` 代表執行日期後二天，用於週五排程帶入週日日期；`-10` 代表前十天）[cite: 1]。
2. **月份偏移擴充（`MONTH_OFFSET`）**：支援任意加減 $N$ 個月（格式可選 `YYYMM` 或 `MM`）。
3. **本週指定星期幾（`WEEK_DAY`）**：依 `BaseDate` 所在週次，自動計算該週星期一（`WEEK_1`）至星期日（`WEEK_7`）的 `YYYMMDD`[cite: 1]。
4. **當月特殊日（`MONTH_EDGE`）**：`MONTH_FIRST`（當月第一天 `YYYMM01`）、`MONTH_LAST`（當月最後一天，自動判定大小月與閏年）、`PREV_MONTH_FIRST`、`PREV_MONTH_LAST`。
5. **自訂樣板組合（`TEMPLATE`）**：允許在字串中嵌入 `{變數代碼}` 進行字串插值（如 `DAVF222.{E1}.FDC.D01` 或 `recv/DAVF*.{E1}.*`）[cite: 1, 6]。
6. **固定字串（`STATIC`）**：直接輸出使用者輸入的固定文字或數字（如 `DAV`、`1`）[cite: 1]。

---

## 5. 排程時間生成器設計（整合圖 2）

1. **年度別（多選 Checkbox）**：預設顯示今年與明年（民國年，如 `115`、`116`），支援動態新增[cite: 7]。
2. **時間區間（單選 Radio）**：`整年`（`01/01` ~ `12/31`）或 `自訂時間區間`（起訖民國日期過濾）[cite: 7]。
3. **預定排程時間（三選一 Radio 模式 + 單次預設）**[cite: 7]：
   * **每月（Monthly）**：最多 3 個指定日（1~31）＋ 1 個「最後一天」Checkbox[cite: 7]。
   * **每週（Weekly）**：一至日 7 個星期 Checkbox（可複選）[cite: 7]。
   * **指定日期（Specific Dates）**：最多 5 個指定日期輸入框（`yyy/MM/dd`）[cite: 7]。
   * **單次執行（Default Single Run）**：沿用群組明細預設日期規則，僅執行一輪[cite: 1]。
4. **預計執行時間（`HH:mm`）**：設定統一時間（如 `13:00`），若群組明細另有指定（如 `11:50`）則以明細為優先[cite: 1, 7]。

---

## 6. HTML 匯入解析規則

讀取 YCM `YCM321-3W2` 網頁原始碼（如 `html.txt`），自動萃取欄位定義[cite: 2]：
1. **基本資訊**：讀取 `<input id="jobCodeNumber">` 與 `<input id="jobName">` 的 `value`[cite: 2]。
2. **工作參數（`#jobParaTb`）**：掃描 `<table id="jobParaTb"> tr.row`，提取 `<th>` 中文名稱與可見輸入元素屬性[cite: 2]。
3. **程式參數（`#programSet`）**：掃描 `<div id="programSet">` 下 `id^="tr_programParameter_"` 的列，排除 `type="hidden"` 欄位後存入 `program_fields`[cite: 2]。

---

## 7. `chromiumoxide` 自動化執行引擎與 GPUI 非同步通訊

### 7.1 執行緒與事件通訊架構
* GPUI 渲染迴圈運行於主執行緒（UI Thread）。
* 當使用者在 GPUI 點擊「執行排程」時，透過 `cx.spawn` 與 `tokio` 啟動背景非同步任務（`browser_runner::execute_group`），並傳入 `mpsc::Sender<RunnerEvent>`。
* 執行引擎每完成一個步驟（登入成功、填完欄位、點擊 F8、取得序號、截圖存檔）[cite: 1]，即發送 `RunnerEvent` 通知 GPUI 呼叫 `cx.notify()` 即時重繪進度條與日誌視窗。

### 7.2 雙軌狀態機執行邏輯
* **軌跡 A — 單一程式多批次（`SINGLE` 模式）**：
  1. 初次進入表單頁後，填寫第 1 筆參數並點擊 `#btnF8`[cite: 1, 2]。
  2. 從第 2 筆起：輪詢等待 `#btnF6`（`F6修改再執行`）變為 Enabled → 點擊 `#btnF6` → 輪詢等待 `#btnF8` 變為 Enabled → 清空並覆寫新一筆的專屬參數與 `#expectExeDate` / `#expectExeTime` → 點擊 `#btnF8`[cite: 1, 2]。
  3. 每次按下 `#btnF8` 後皆從 `#OpMsg` 擷取工作序號並呼叫 `page.save_screenshot()` 存證[cite: 1, 2]。
* **軌跡 B — 多程式串接（`MULTI` 模式）**：
  1. 依 `step_order` 逐支導航至 `getSysData` → `#fuzzyInquiry` 查詢 → `#btnF4` → 選取 `#SampleGrid` 第 1 筆資料列 → `#btnF1` 新增[cite: 1]。
  2. 驗證 `#jobCodeNumber` 正確載入後，填入該程式專屬參數與共同欄位；若勾選 `link_prev_sn`，則將上一支程式從 `#OpMsg` 取得的工作序號填入 `#preJobSn1`[cite: 1]。
  3. 點擊 `#btnF8` 並截圖存證，循環至群組內所有程式完成[cite: 1]。

---

## 8. 建議 Rust 專案目錄結構（GPUI 版）

```text
ycm_batch_scheduler/
├── Cargo.toml                  # 依賴宣告 (gpui, gpui-component, chromiumoxide, rusqlite, scraper, tokio, chrono)
├── ycm_scheduler.db            # SQLite 資料庫 (首次啟動自動初始化建表)
├── screenshots/                # 自動排程截圖存檔目錄[cite: 1]
└── src/
    ├── main.rs                 # GPUI Application 初始化與主視窗註冊
    ├── db/
    │   ├── mod.rs
    │   ├── schema.rs           # SQLite 建表與預設設定/種子資料初始化
    │   └── repository.rs       # 程式、欄位、群組之 CRUD 操作
    ├── parser/
    │   └── html_parser.rs      # 解析 YCM 網頁 HTML 萃取出專屬欄位[cite: 2]
    ├── engine/
    │   ├── param_resolver.rs   # 圖 1 (E1~EG) 與擴充動態日期參數運算引擎[cite: 6]
    │   ├── schedule_gen.rs     # 圖 2 排程時間定義展開器[cite: 7]
    │   └── browser_runner.rs   # chromiumoxide Edge 控制與 F8/F6 雙軌狀態機[cite: 1, 2]
    └── ui/
        ├── mod.rs
        ├── app_workspace.rs    # 主視窗雙欄佈局與側邊導航狀態管理
        ├── program_view.rs     # 批次程式管理與 HTML 匯入介面
        ├── group_view.rs       # 群組設定、共同項目與圖 1 參數規則編輯器
        ├── schedule_view.rs    # 圖 2 排程時間生成器與預覽清單介面
        └── runner_view.rs      # 自動化執行監控台與即時日誌介面