# 第一版驗證紀錄

日期：2026-10-29。建置：Rust 1.96.0 stable，x86_64-pc-windows-msvc，release，靜態 C 執行階段。

## 已通過

- `cargo test --offline`：4 個測試通過。
- `cargo clippy --offline --all-targets -- -D warnings`：通過。
- `cargo fmt --check`：通過。
- `cargo build --release --offline`：成功產出原生 exe。
- 一般 Windows 桌面啟動：系統匣建立成功，程序持續運行；測試後已停止程序。

## 初步資源量測

Windows x64 開發機（NT 10.0.26300.0；Intel64 Family 6 Model 154 Stepping 3），release 版本，啟動後等待 2 秒，再觀察閒置 10 秒。設定畫面與提醒視窗皆未開啟；未啟用自啟動。

| 指標 | 本次數據 |
| --- | ---: |
| exe 大小 | 245.5 KiB |
| Working set | 23.58 MiB |
| Private memory | 8.95 MiB |
| 10 秒內程序 CPU 時間增量 | 0 ms（系統計數器精度內） |

這是單次短測，不代表長期平均、所有電腦或提醒視窗開啟時的數據。尚未量測背景喚醒頻率與精確啟動時間，也尚未訂定資源驗收門檻。

執行工具的隔離桌面無法建立系統匣；改在一般桌面執行後成功。程式無法建立系統匣時會提示錯誤並以失敗狀態退出，不會留下沒有操作入口的背景程序。

## 尚需手動驗收

提醒視窗外觀、焦點、確認與 Alt+F4 行為；設定視窗操作與設定持久化；真實登入自啟動；鎖定／解鎖；睡眠／喚醒；Explorer 重啟；多螢幕與不同 DPI。詳細步驟見 README。
