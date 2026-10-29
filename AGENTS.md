# 專案開發指南

## 目標與技術

這是僅支援 Windows 10 / 11 x64 的低資源健康提醒工具。使用 Rust stable（MSVC）與 `windows-sys` 呼叫 Win32 API。維持原生系統匣與視窗架構，避免引入瀏覽器、Web server、.NET、背景輪詢或非必要依賴。

以繁體中文撰寫使用者介面與文件。修改前先閱讀 `docs/spec.md`；驗證現況見 `docs/validation.md`。不要把尚未執行的手動驗收寫成已通過，也不要將單次資源量測當成效能保證。

## 程式結構

- `src/main.rs`：進入點、錯誤顯示與退出狀態。
- `src/model.rs`：設定序列化、提醒排程與單元測試，不依賴 Windows UI。
- `src/win.rs`：Win32 訊息迴圈、系統匣、視窗、登入自啟動及鎖定／電源事件。
- `.cargo/config.toml`：x64 MSVC 靜態 C 執行階段設定，必須保留在版本控制。
- `Cargo.lock`：此專案是應用程式，需納入版本控制。

## 不可破壞的行為

- 每種提醒最多一筆待確認項目；各項獨立確認，確認後重新計時。
- 提醒持續置頂，不搶鍵盤焦點，不阻擋其他程式；Escape 與 Alt+F4 不得確認或關閉提醒。
- 暫停不清除已待確認項目；恢復後不補發累積提醒。
- 鎖定或睡眠期間不新增提醒；恢復後重新計時，保留先前待確認項目。
- 停用某項可清除該項提醒；修改其他項目不重設無關的計時器。
- 背景僅安排下一次到期事件，避免固定每秒喚醒。
- 同一使用者工作階段僅有一個執行個體。
- 設定僅存本機；自啟動預設關閉，由使用者選擇，不要求管理員權限。

## Win32 安全與生命週期

`App` 狀態透過 `GWLP_USERDATA` 提供給同執行緒視窗 callback。新增 API 呼叫時注意同步重入，不可跨可能重入的呼叫保留可變參照。UTF-16 buffer 必須活到原生 API 使用結束；不得儲存指向暫時 `Vec` 的指標。正確釋放 menu、registry key、mutex 等資源，保持退出時清理系統匣及通知註冊。

## 建置與驗證

在 Windows 安裝 Rust MSVC 工具鏈與 Microsoft C++ Build Tools（含 Windows SDK）。例行指令：

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
```

無網路但套件已快取時可加 `--offline`。成品位於 `target/release/wake-up-drink-water.exe`，只需交付 exe；使用者不需要 Rust 或 .NET。

排程邏輯變更應以注入 `Instant` 的方式測試，不使用實際等待。UI、鎖定／喚醒、登入自啟動等修改依 README 的手動驗收清單測試。文件或忽略規則修改不需新增測試。

啟動測試不得未經使用者選擇就開啟自啟動或覆寫既有個人設定。若隔離桌面無法建立系統匣，應記錄環境限制，不要刪除錯誤處理以掩蓋問題。

## 版本控制

不提交 `target/`、`dist/`、IDE 設定、記錄檔或 crash dump。保留使用者既有變更，不自動提交或推送。修改功能時同步更新規格、使用說明及必要驗證紀錄。
