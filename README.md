# 健康提醒工具

Rust + Win32 的 Windows 10 / 11 x64 系統匣工具，提醒喝水、起身走動、眨眼。不需要安裝 Rust 或 .NET 即可執行成品。

## 取得與執行

建置後只需複製 `target/release/wake-up-drink-water.exe` 到固定資料夾並執行，不需要複製整個 `target` 目錄，也不需另外安裝 Visual C++ 執行階段。成品已靜態連結 C 執行階段；目前不提供 ARM64 或 32 位元版本。

工作管理員「詳細資料」中的程序名稱是 `wake-up-drink-water.exe`。首次啟動不會自動啟用登入自啟動。

## 使用

執行 `target/release/wake-up-drink-water.exe` 後會出現系統匣資訊圖示，主視窗不會自動開啟。圖示可能收在 Windows 系統匣的隱藏區域。

- 雙擊圖示，或右鍵選「設定」：調整各項間隔、啟用狀態與登入自啟動。
- 右鍵選「暫停提醒／恢復提醒」：暫停不清除已出現的提醒，恢復後重新計時。
- 右鍵選「退出」：完全結束程式。
- 提醒會持續置頂，按對應「我知道了」才消失，不搶走其他程式的鍵盤焦點。
- 「預覽提醒」會立即產生所有已啟用項目的待確認提醒，方便檢查效果。

預設間隔：喝水 30 分鐘、走動 60 分鐘、眨眼 20 分鐘；無聲音。間隔範圍為 1–1440 分鐘。

多項提醒合併顯示，各項獨立確認。未確認期間不重複堆積，確認後才重新計時。休眠或鎖定期間不新增提醒，恢復使用時不補發累積提醒。全螢幕與會議不自動暫停，請使用手動暫停。

設定存在 `%LOCALAPPDATA%\WakeUpDrinkWater\settings.txt`。自啟動使用目前使用者的 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\WakeUpDrinkWater`，不需要系統管理員權限。啟用前請將 exe 放到固定位置；移動 exe 後需重新儲存已勾選的自啟動設定。

移除前，先取消自啟動並儲存，再退出程式與刪除 exe；如不需保留設定，可刪除上述設定資料夾。

## 開發與建置

開發機需 Rust stable（MSVC 工具鏈）與 Microsoft C++ Build Tools 的「使用 C++ 的桌面開發」（含 Windows SDK）。

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
```

首次建置會由 Cargo 下載依賴；已有快取且不連網時，可加上 `--offline`。專案包含 Cargo.lock；唯一直接依賴為 windows-sys。x64 MSVC 建置已設定靜態 C 執行階段。

背景使用 Windows 訊息迴圈與下一次到期計時器，無網路、隱藏瀏覽器或每秒輪詢。實際資源使用依環境量測，不保證尚未訂定的記憶體門檻。

## 驗收

自動測試涵蓋提醒獨立確認、未確認去重、暫停／離開後不補發、設定驗證及停用行為。

Windows 實機仍需驗收：

- 預覽提醒不搶輸入焦點，Alt+F4／Escape 不關閉提醒，三項可分別確認。
- 儲存設定、重新啟動，確認設定保留；無效間隔會提示且不儲存。
- 暫停後新提醒不出現，已出現提醒仍在；恢復不補發。
- 鎖定／解鎖與睡眠／喚醒後不補發，已待確認項目保留。
- 啟用自啟動後重新登入，確認只有一個執行個體，且不自動開啟設定畫面。
- 重啟 Explorer 後系統匣圖示恢復；多螢幕與不同 DPI 下視窗可讀、可操作。

## 專案文件

- [功能規格](docs/spec.md)
- [驗證紀錄與初步資源量測](docs/validation.md)
- [開發代理指南](AGENTS.md)

初步量測：release exe 約 245.5 KiB，閒置 working set 約 23.58 MiB；10 秒內 CPU 時間增量為 0 ms（計數器精度內）。這是單次短測，詳細條件及尚未完成的驗收見驗證紀錄。

Git 忽略建置產物、IDE 設定與本機記錄檔；`Cargo.lock` 與 `.cargo/config.toml` 需保留。
