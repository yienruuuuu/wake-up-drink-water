use crate::model::{Scheduler, Settings, NAMES};
use std::{
    ffi::c_void,
    mem::{size_of, zeroed},
    path::PathBuf,
    ptr::{null, null_mut},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::*, Registry::*, RemoteDesktop::*, Threading::*},
    UI::{Controls::*, Shell::*, WindowsAndMessaging::*},
};

const TRAY: u32 = WM_APP + 1;
const REOPEN: u32 = WM_APP + 2;
const SETTINGS: usize = 10;
const PAUSE: usize = 11;
const EXIT: usize = 12;
const SAVE: usize = 20;
const PREVIEW: usize = 21;
const TIMER: usize = 1;
const CLASS: &str = "WakeUpDrinkWater.Window.v1";
const RUN_NAME: &str = "WakeUpDrinkWater";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

struct App {
    scheduler: Scheduler,
    owner: HWND,
    reminder: HWND,
    settings: HWND,
    buttons: [HWND; 3],
    checks: [HWND; 3],
    edits: [HWND; 3],
    startup: HWND,
    taskbar_created: u32,
    locked: bool,
    suspended: bool,
    config: PathBuf,
}

// Windows delivers these callbacks on the creating thread. No references to App
// are retained while pumping messages or creating/destroying top-level windows.
unsafe fn app_ptr(hwnd: HWND) -> *mut App {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App
}

pub fn show_error(message: &str) {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(message).as_ptr(),
            wide("健康提醒工具").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn run() -> Result<(), String> {
    unsafe {
        let mutex = CreateMutexW(null(), 0, wide("Local\\WakeUpDrinkWater.v1").as_ptr());
        if mutex.is_null() {
            return Err("無法建立單一執行個體鎖".into());
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let existing = FindWindowW(wide(CLASS).as_ptr(), wide("健康提醒工具背景程式").as_ptr());
            if !existing.is_null() {
                PostMessageW(existing, REOPEN, 0, 0);
            }
            CloseHandle(mutex);
            return Ok(());
        }
        let result = run_loop();
        CloseHandle(mutex);
        result
    }
}

unsafe fn run_loop() -> Result<(), String> {
    let config = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("找不到 LOCALAPPDATA，無法儲存設定")?
        .join("WakeUpDrinkWater")
        .join("settings.txt");
    let mut settings = match std::fs::read_to_string(&config) {
        Ok(text) => {
            Settings::decode(&text).map_err(|e| format!("{e}\n設定檔：{}", config.display()))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(e) => return Err(format!("無法讀取設定：{e}")),
    };
    // Registry is authoritative, including changes made outside the app.
    settings.startup = startup_enabled();
    let instance = GetModuleHandleW(null());
    let class_name = wide(CLASS);
    let wc = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        hIcon: LoadIconW(null_mut(), IDI_INFORMATION),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
        lpszClassName: class_name.as_ptr(),
        ..zeroed()
    };
    if RegisterClassW(&wc) == 0 {
        return Err("無法註冊視窗".into());
    }
    let mut app = Box::new(App {
        scheduler: Scheduler::new(settings, Instant::now()),
        owner: null_mut(),
        reminder: null_mut(),
        settings: null_mut(),
        buttons: [null_mut(); 3],
        checks: [null_mut(); 3],
        edits: [null_mut(); 3],
        startup: null_mut(),
        taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
        locked: false,
        suspended: false,
        config,
    });
    let ptr = &mut *app as *mut App;
    let owner = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        wide(CLASS).as_ptr(),
        wide("健康提醒工具背景程式").as_ptr(),
        0,
        0,
        0,
        0,
        0,
        null_mut(),
        null_mut(),
        instance,
        ptr as *const c_void,
    );
    if owner.is_null() {
        return Err("無法建立背景視窗".into());
    }
    app.owner = owner;
    if WTSRegisterSessionNotification(owner, NOTIFY_FOR_THIS_SESSION) == 0 {
        DestroyWindow(owner);
        return Err("無法註冊 Windows 鎖定狀態通知".into());
    }
    // Query the current state so starting while locked does not schedule alerts.
    let mut buffer = null_mut();
    let mut bytes = 0;
    if WTSQuerySessionInformationW(
        WTS_CURRENT_SERVER_HANDLE,
        WTS_CURRENT_SESSION,
        WTSSessionInfoEx,
        &mut buffer,
        &mut bytes,
    ) != 0
    {
        if bytes as usize >= size_of::<WTSINFOEXW>() {
            let info = &*(buffer as *const WTSINFOEXW);
            if info.Level == 1 {
                app.locked = info.Data.WTSInfoExLevel1.SessionFlags == 0;
            }
        }
        WTSFreeMemory(buffer as *mut c_void);
    }
    app.scheduler.away = app.locked;
    if !tray(&app, NIM_ADD) {
        DestroyWindow(owner);
        return Err("無法建立系統匣圖示".into());
    }
    schedule(&app);
    let mut msg: MSG = zeroed();
    loop {
        let status = GetMessageW(&mut msg, null_mut(), 0, 0);
        if status == 0 {
            break;
        }
        if status == -1 {
            DestroyWindow(owner);
            return Err("Windows 訊息迴圈失敗".into());
        }
        if !app.settings.is_null() && IsDialogMessageW(app.settings, &msg) != 0 {
            continue;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    Ok(())
}

unsafe fn tray(app: &App, operation: u32) -> bool {
    let mut data: NOTIFYICONDATAW = zeroed();
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = app.owner;
    data.uID = 1;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = TRAY;
    data.hIcon = LoadIconW(null_mut(), IDI_INFORMATION);
    let tip = wide(if app.scheduler.paused {
        "健康提醒工具（已暫停）"
    } else {
        "健康提醒工具"
    });
    data.szTip[..tip.len()].copy_from_slice(&tip);
    Shell_NotifyIconW(operation, &data) != 0
}

unsafe fn schedule(app: &App) {
    KillTimer(app.owner, TIMER);
    if let Some(delay) = app.scheduler.next_delay(Instant::now()) {
        let ms = delay.as_millis().clamp(10, 0x7fff_ffff) as u32;
        SetTimer(app.owner, TIMER, ms, None);
    }
}

unsafe fn child(
    parent: HWND,
    class: &str,
    text: &str,
    style: u32,
    rect: (i32, i32, i32, i32),
    id: usize,
) -> HWND {
    let h = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        rect.0,
        rect.1,
        rect.2,
        rect.3,
        parent,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(h, WM_SETFONT, GetStockObject(DEFAULT_GUI_FONT) as usize, 1);
    h
}

unsafe fn open_settings(ptr: *mut App) {
    if !(*ptr).settings.is_null() {
        ShowWindow((*ptr).settings, SW_SHOWNORMAL);
        SetForegroundWindow((*ptr).settings);
        return;
    }
    let h = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        wide(CLASS).as_ptr(),
        wide("健康提醒設定").as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        440,
        350,
        (*ptr).owner,
        null_mut(),
        GetModuleHandleW(null()),
        ptr as *const c_void,
    );
    (*ptr).settings = h;
    if h.is_null() {
        show_error("無法建立設定視窗");
        return;
    }
    child(
        h,
        "STATIC",
        "提醒間隔（1–1440 分鐘）",
        0,
        (24, 18, 360, 24),
        0,
    );
    for (i, name) in NAMES.iter().enumerate() {
        let y = 54 + i as i32 * 42;
        (*ptr).checks[i] = child(
            h,
            "BUTTON",
            name,
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
            (24, y, 160, 26),
            100 + i,
        );
        SendMessageW(
            (*ptr).checks[i],
            BM_SETCHECK,
            (*ptr).scheduler.settings.enabled[i] as usize,
            0,
        );
        (*ptr).edits[i] = child(
            h,
            "EDIT",
            &(*ptr).scheduler.settings.minutes[i].to_string(),
            WS_BORDER | WS_TABSTOP | ES_NUMBER as u32 | ES_AUTOHSCROLL as u32,
            (210, y, 80, 26),
            110 + i,
        );
        SendMessageW((*ptr).edits[i], EM_SETLIMITTEXT, 4, 0);
        child(h, "STATIC", "分鐘", 0, (304, y + 3, 60, 24), 0);
    }
    (*ptr).startup = child(
        h,
        "BUTTON",
        "登入 Windows 時自動啟動",
        BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
        (24, 186, 360, 26),
        120,
    );
    SendMessageW(
        (*ptr).startup,
        BM_SETCHECK,
        (*ptr).scheduler.settings.startup as usize,
        0,
    );
    child(
        h,
        "BUTTON",
        "預覽提醒",
        WS_TABSTOP,
        (24, 236, 110, 32),
        PREVIEW,
    );
    child(
        h,
        "BUTTON",
        "儲存設定",
        WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        (264, 236, 120, 32),
        SAVE,
    );
    ShowWindow(h, SW_SHOW);
    SetForegroundWindow(h);
}

unsafe fn sync_reminder(ptr: *mut App) {
    let show = (*ptr).scheduler.pending.iter().any(|p| *p) && !(*ptr).scheduler.away;
    if !show {
        if !(*ptr).reminder.is_null() {
            ShowWindow((*ptr).reminder, SW_HIDE);
        }
        return;
    }
    if (*ptr).reminder.is_null() {
        let mut work: RECT = zeroed();
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as *mut c_void, 0);
        let h = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            wide(CLASS).as_ptr(),
            wide("該照顧自己一下了").as_ptr(),
            WS_CAPTION,
            (work.right - 400).max(work.left),
            (work.bottom - 250).max(work.top),
            380,
            220,
            (*ptr).owner,
            null_mut(),
            GetModuleHandleW(null()),
            ptr as *const c_void,
        );
        if h.is_null() {
            show_error("無法建立提醒視窗，請從系統匣退出後重新啟動");
            return;
        }
        (*ptr).reminder = h;
        child(
            h,
            "STATIC",
            "完成後按下對應按鈕確認",
            0,
            (20, 16, 330, 24),
            0,
        );
        for (i, name) in NAMES.iter().enumerate() {
            (*ptr).buttons[i] = child(
                h,
                "BUTTON",
                &format!("{name} — 我知道了"),
                WS_TABSTOP,
                (20, 48 + i as i32 * 40, 330, 32),
                200 + i,
            );
        }
    }
    for i in 0..3 {
        ShowWindow(
            (*ptr).buttons[i],
            if (*ptr).scheduler.pending[i] {
                SW_SHOWNA
            } else {
                SW_HIDE
            },
        );
    }
    SetWindowPos(
        (*ptr).reminder,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
}

unsafe fn save_settings(ptr: *mut App) -> Result<(), String> {
    let mut settings = (*ptr).scheduler.settings.clone();
    for i in 0..3 {
        let mut value = [0u16; 32];
        let len = GetWindowTextW((*ptr).edits[i], value.as_mut_ptr(), value.len() as i32);
        settings.minutes[i] = String::from_utf16_lossy(&value[..len as usize])
            .trim()
            .parse()
            .map_err(|_| "請輸入整數分鐘")?;
        if !(1..=1440).contains(&settings.minutes[i]) {
            return Err("提醒間隔必須介於 1 到 1440 分鐘".into());
        }
        settings.enabled[i] =
            SendMessageW((*ptr).checks[i], BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
    }
    settings.startup = SendMessageW((*ptr).startup, BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
    let parent = (*ptr).config.parent().unwrap();
    std::fs::create_dir_all(parent).map_err(|e| format!("無法建立設定資料夾：{e}"))?;
    let temp = parent.join("settings.tmp");
    std::fs::write(&temp, settings.encode()).map_err(|e| format!("無法寫入設定：{e}"))?;
    let old_startup = startup_enabled();
    set_startup(settings.startup)?;
    if let Err(e) = std::fs::rename(&temp, &(*ptr).config) {
        let rollback = set_startup(old_startup);
        return Err(format!(
            "無法儲存設定：{e}{}",
            if rollback.is_err() {
                "；自啟動設定回復失敗，請檢查 Windows 啟動設定"
            } else {
                ""
            }
        ));
    }
    (*ptr).scheduler.apply(settings, Instant::now());
    sync_reminder(ptr);
    schedule(&*ptr);
    DestroyWindow((*ptr).settings);
    Ok(())
}

unsafe fn startup_enabled() -> bool {
    let mut key = null_mut();
    if RegOpenKeyExW(
        HKEY_CURRENT_USER,
        wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run").as_ptr(),
        0,
        KEY_QUERY_VALUE,
        &mut key,
    ) != ERROR_SUCCESS
    {
        return false;
    }
    let mut kind = 0;
    let mut bytes = 0;
    let result = RegQueryValueExW(
        key,
        wide(RUN_NAME).as_ptr(),
        null(),
        &mut kind,
        null_mut(),
        &mut bytes,
    );
    RegCloseKey(key);
    result == ERROR_SUCCESS && kind == REG_SZ && bytes > 2
}

unsafe fn set_startup(enabled: bool) -> Result<(), String> {
    let path = std::env::current_exe().map_err(|e| format!("無法取得程式路徑：{e}"))?;
    use std::os::windows::ffi::OsStrExt;
    let mut command = vec![b'"' as u16];
    command.extend(path.as_os_str().encode_wide());
    command.extend([b'"' as u16, 0]);
    let mut key = null_mut();
    let status = RegCreateKeyExW(
        HKEY_CURRENT_USER,
        wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run").as_ptr(),
        0,
        null(),
        REG_OPTION_NON_VOLATILE,
        KEY_SET_VALUE,
        null(),
        &mut key,
        null_mut(),
    );
    if status != ERROR_SUCCESS {
        return Err(format!("無法開啟自啟動設定（{status}）"));
    }
    let status = if enabled {
        RegSetValueExW(
            key,
            wide(RUN_NAME).as_ptr(),
            0,
            REG_SZ,
            command.as_ptr() as *const u8,
            (command.len() * 2) as u32,
        )
    } else {
        RegDeleteValueW(key, wide(RUN_NAME).as_ptr())
    };
    RegCloseKey(key);
    if status == ERROR_SUCCESS || (!enabled && status == ERROR_FILE_NOT_FOUND) {
        Ok(())
    } else {
        Err(format!("無法修改自啟動設定（{status}）"))
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, w: usize, l: isize) -> isize {
    if message == WM_NCCREATE {
        let create = &*(l as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let ptr = app_ptr(hwnd);
    if ptr.is_null() {
        return DefWindowProcW(hwnd, message, w, l);
    }
    if message == (*ptr).taskbar_created && message != 0 {
        tray(&*ptr, NIM_ADD);
        return 0;
    }
    match message {
        WM_TIMER if hwnd == (*ptr).owner => {
            (*ptr).scheduler.tick(Instant::now());
            sync_reminder(ptr);
            schedule(&*ptr);
            0
        }
        REOPEN => {
            open_settings(ptr);
            0
        }
        TRAY => {
            if l as u32 == WM_LBUTTONDBLCLK {
                open_settings(ptr);
            } else if l as u32 == WM_RBUTTONUP || l as u32 == WM_CONTEXTMENU {
                let menu = CreatePopupMenu();
                AppendMenuW(menu, MF_STRING, SETTINGS, wide("設定").as_ptr());
                AppendMenuW(
                    menu,
                    MF_STRING,
                    PAUSE,
                    wide(if (*ptr).scheduler.paused {
                        "恢復提醒"
                    } else {
                        "暫停提醒"
                    })
                    .as_ptr(),
                );
                AppendMenuW(menu, MF_SEPARATOR, 0, null());
                AppendMenuW(menu, MF_STRING, EXIT, wide("退出").as_ptr());
                let mut point: POINT = zeroed();
                GetCursorPos(&mut point);
                SetForegroundWindow(hwnd);
                let command = TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    point.x,
                    point.y,
                    0,
                    hwnd,
                    null(),
                );
                DestroyMenu(menu);
                PostMessageW(hwnd, WM_NULL, 0, 0);
                if command != 0 {
                    PostMessageW(hwnd, WM_COMMAND, command as usize, 0);
                }
            }
            0
        }
        WM_COMMAND => {
            let id = w & 0xffff;
            match id {
                SETTINGS => open_settings(ptr),
                PAUSE => {
                    (*ptr).scheduler.paused = !(*ptr).scheduler.paused;
                    if !(*ptr).scheduler.paused {
                        (*ptr).scheduler.reset(Instant::now());
                    }
                    tray(&*ptr, NIM_MODIFY);
                    schedule(&*ptr);
                }
                EXIT => {
                    DestroyWindow((*ptr).owner);
                }
                SAVE => {
                    if let Err(e) = save_settings(ptr) {
                        show_error(&e);
                    }
                }
                PREVIEW => {
                    for i in 0..3 {
                        if (*ptr).scheduler.settings.enabled[i] {
                            (*ptr).scheduler.pending[i] = true;
                        }
                    }
                    sync_reminder(ptr);
                    schedule(&*ptr);
                }
                200..=202 => {
                    (*ptr).scheduler.acknowledge(id - 200, Instant::now());
                    sync_reminder(ptr);
                    schedule(&*ptr);
                }
                _ => {}
            }
            0
        }
        WM_WTSSESSION_CHANGE => {
            if w == WTS_SESSION_LOCK as usize {
                (*ptr).locked = true;
            }
            if w == WTS_SESSION_UNLOCK as usize {
                (*ptr).locked = false;
            }
            update_away(ptr);
            0
        }
        WM_POWERBROADCAST => {
            if w == PBT_APMSUSPEND as usize {
                (*ptr).suspended = true;
                update_away(ptr);
            }
            if w == PBT_APMRESUMEAUTOMATIC as usize || w == PBT_APMRESUMESUSPEND as usize {
                (*ptr).suspended = false;
                update_away(ptr);
            }
            1
        }
        WM_CLOSE if hwnd == (*ptr).reminder => 0,
        WM_CLOSE if hwnd == (*ptr).owner => 0,
        WM_DESTROY => {
            if hwnd == (*ptr).settings {
                (*ptr).settings = null_mut();
            }
            if hwnd == (*ptr).reminder {
                (*ptr).reminder = null_mut();
            }
            if hwnd == (*ptr).owner {
                KillTimer(hwnd, TIMER);
                WTSUnRegisterSessionNotification(hwnd);
                tray(&*ptr, NIM_DELETE);
                if !(*ptr).reminder.is_null() {
                    DestroyWindow((*ptr).reminder);
                }
                if !(*ptr).settings.is_null() {
                    DestroyWindow((*ptr).settings);
                }
                PostQuitMessage(0);
            }
            0
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            DefWindowProcW(hwnd, message, w, l)
        }
        _ => DefWindowProcW(hwnd, message, w, l),
    }
}

unsafe fn update_away(ptr: *mut App) {
    let away = (*ptr).locked || (*ptr).suspended;
    let was_away = (*ptr).scheduler.away;
    (*ptr).scheduler.away = away;
    if was_away && !away {
        (*ptr).scheduler.reset(Instant::now());
    }
    sync_reminder(ptr);
    schedule(&*ptr);
}
