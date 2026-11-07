use serde_json::Value;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::Duration;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::keys::KEYS;
use crate::{w, HIDE, INHIBIT, MAIN, PARENT};

pub static WM_INFO: Mutex<(String, String, i32, i32)> = Mutex::new((String::new(), String::new(), 0, 0));

pub fn scan(name: &str) -> Option<u16> {
    let n = name.trim().to_lowercase();
    KEYS.iter().find(|(k, _)| *k == n).map(|(_, c)| *c)
}

fn press(code: u16, down: bool) {
    let mut i: INPUT = unsafe { std::mem::zeroed() };
    i.r#type = INPUT_KEYBOARD;
    let mut flags = KEYEVENTF_SCANCODE;
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    if code > 255 {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    i.Anonymous.ki = KEYBDINPUT { wVk: 0, wScan: code, dwFlags: flags, time: 0, dwExtraInfo: crate::OWN_INPUT };
    unsafe { SendInput(1, &i, size_of::<INPUT>() as i32) };
}

fn vk(k: u16, up: bool) {
    let mut i: INPUT = unsafe { std::mem::zeroed() };
    i.r#type = INPUT_KEYBOARD;
    i.Anonymous.ki = KEYBDINPUT { wVk: k, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: crate::OWN_INPUT };
    unsafe { SendInput(1, &i, size_of::<INPUT>() as i32) };
}

pub fn mask_alt() {
    vk(0xE8, false);
    vk(0xE8, true);
}

pub fn shortcut(s: &str) -> Option<(u32, u32)> {
    let mut mods = MOD_NOREPEAT;
    let mut key = None;
    for p in s.split('+') {
        match p.trim().to_lowercase().as_str() {
            "control" | "ctrl" | "commandorcontrol" | "cmdorctrl" => mods |= MOD_CONTROL,
            "shift" => mods |= MOD_SHIFT,
            "alt" | "option" => mods |= MOD_ALT,
            "super" | "meta" | "win" | "command" | "cmd" => mods |= MOD_WIN,
            k => key = vk_of(k),
        }
    }
    key.map(|k| (mods, k as u32))
}

fn vk_of(k: &str) -> Option<u16> {
    if k.len() == 1 {
        let c = k.as_bytes()[0].to_ascii_uppercase();
        if c.is_ascii_alphanumeric() {
            return Some(c as u16);
        }
    }
    if let Some(d) = k.strip_prefix('f').and_then(|d| d.parse::<u16>().ok()) {
        return Some(0x6F + d);
    }
    if let Some(d) = k.strip_prefix("num").and_then(|d| d.parse::<u16>().ok()) {
        return Some(0x60 + d);
    }
    Some(match k {
        "space" => 0x20,
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "backspace" => 0x08,
        "delete" => 0x2E,
        "insert" => 0x2D,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "up" | "arrowup" => 0x26,
        "down" | "arrowdown" => 0x28,
        "left" | "arrowleft" => 0x25,
        "right" | "arrowright" => 0x27,
        "escape" | "esc" => 0x1B,
        "plus" => 0xBB,
        "-" | "minus" => 0xBD,
        "`" => 0xC0,
        _ => return None,
    })
}

fn fill(s: &str, clean: impl Fn(&str) -> String) -> String {
    let info = WM_INFO.lock().unwrap().clone();
    s.replace("{{app_name}}", &clean(&info.0))
        .replace("{{window_name}}", &clean(&info.1))
        .replace("{{pointer_x}}", &info.2.to_string())
        .replace("{{pointer_y}}", &info.3.to_string())
}

fn expand(s: &str) -> String {
    fill(s, |value| value.to_string())
}

fn expand_command(s: &str) -> String {
    fill(s, |value| value.chars().filter(|c| c.is_alphanumeric() || " .,-_:+#@[]/\\".contains(*c)).collect())
}

pub fn open_path(p: &str) {
    let op = w("open");
    let p = w(p);
    unsafe { ShellExecuteW(null_mut(), op.as_ptr(), p.as_ptr(), null(), null(), SW_SHOWNORMAL) };
}

fn clipboard(text: &str) {
    let data = w(text);
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return;
        }
        EmptyClipboard();
        let h = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2);
        if h.is_null() {
            CloseClipboard();
            return;
        }
        let p = GlobalLock(h) as *mut u16;
        if p.is_null() {
            GlobalFree(h);
            CloseClipboard();
            return;
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
        GlobalUnlock(h);
        if SetClipboardData(13, h as HANDLE).is_null() {
            GlobalFree(h);
        }
        CloseClipboard();
    }
}

pub fn exe_name(pid: u32) -> String {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return String::new();
        }
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n);
        CloseHandle(h);
        let full = String::from_utf16_lossy(&buf[..n as usize]);
        let file = full.rsplit('\\').next().unwrap_or("").to_string();
        match file.rfind('.') {
            Some(i) => file[..i].to_string(),
            None => file,
        }
    }
}

pub fn title(h: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn matches(cond: &str, value: &str) -> bool {
    if cond.is_empty() {
        return true;
    }
    let c = cond.trim_matches('/').to_lowercase();
    value.to_lowercase().contains(&c)
}

static FOCUS: Mutex<(String, String, isize)> = Mutex::new((String::new(), String::new(), 0));

unsafe extern "system" fn each(h: HWND, _: LPARAM) -> BOOL {
    if IsWindowVisible(h) == 0 {
        return 1;
    }
    let mut pid = 0;
    GetWindowThreadProcessId(h, &mut pid);
    let mut f = FOCUS.lock().unwrap();
    let t = title(h);
    if !t.is_empty() && matches(&f.0, &exe_name(pid)) && matches(&f.1, &t) {
        f.2 = h as isize;
        return 0;
    }
    1
}

fn focus(app: &str, window: &str) {
    *FOCUS.lock().unwrap() = (app.to_string(), window.to_string(), 0);
    unsafe {
        EnumWindows(Some(each), 0);
        let h = FOCUS.lock().unwrap().2 as HWND;
        if !h.is_null() {
            if IsIconic(h) != 0 {
                ShowWindow(h, SW_RESTORE);
            }
            SetForegroundWindow(h);
        }
    }
}

fn keys(seq: &[(u16, bool, u64)]) {
    for &(code, down, delay) in seq {
        if delay > 0 {
            sleep(Duration::from_millis(delay));
        }
        press(code, down);
    }
}

fn is_script(command: &str) -> bool {
    let command = command.trim_start();
    let first = match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(""),
        None => command.split_whitespace().next().unwrap_or(""),
    };
    let first = first.to_ascii_lowercase();
    first.ends_with(".bat") || first.ends_with(".cmd")
}

pub fn risky(action: &Value) -> Option<String> {
    let s = |k: &str| action[k].as_str().unwrap_or("").to_string();
    match action["type"].as_str()? {
        "execute-command" => Some(s("command")),
        "open-file" => Some(s("path")),
        "open-uri" => {
            let uri = s("uri");
            let scheme = uri.split_once(':').map(|(s, _)| s.to_ascii_lowercase()).unwrap_or_default();
            (!matches!(scheme.as_str(), "http" | "https" | "mailto")).then_some(uri)
        }
        "send-websocket-message" => Some(format!("{} {}", s("url"), s("message"))),
        "simulate-hotkey" => Some(s("hotkey")),
        "execute-macro" => Some(action["macro"].to_string()),
        _ => None,
    }
}

static TRUSTED: Mutex<Option<std::collections::HashSet<String>>> = Mutex::new(None);

fn trusted_path() -> std::path::PathBuf {
    crate::menu::dir().join("trusted-actions.json")
}

fn allowed(main: HWND, detail: &str, ask: &(String, String)) -> bool {
    let mut guard = TRUSTED.lock().unwrap();
    let trusted = guard.get_or_insert_with(|| {
        let text = std::fs::read_to_string(trusted_path()).unwrap_or_default();
        serde_json::from_str::<Vec<String>>(text.trim_start_matches('\u{feff}')).unwrap_or_default().into_iter().collect()
    });
    if trusted.contains(detail) {
        return true;
    }
    unsafe {
        SendMessageW(main, HIDE, 0, 0);
        for _ in 0..100 {
            if IsWindowVisible(main) == 0 {
                break;
            }
            sleep(Duration::from_millis(5));
        }
        let shown: String = detail.chars().take(600).collect();
        let text = w(&ask.1.replace("{{actions}}", &shown));
        let title = w(&ask.0);
        let flags = MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2 | MB_TOPMOST | MB_SETFOREGROUND;
        if MessageBoxW(null_mut(), text.as_ptr(), title.as_ptr(), flags) != IDYES {
            return false;
        }
    }
    trusted.insert(detail.to_string());
    let mut list: Vec<&String> = trusted.iter().collect();
    list.sort();
    if let Ok(text) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(trusted_path(), text);
    }
    true
}

pub fn run(actions: &[Value], ask: Option<&(String, String)>) {
    let main = MAIN.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    let mut inhibited = false;

    for a in actions {
        if let (Some(ask), Some(detail)) = (ask, risky(a)) {
            if !allowed(main, &detail, ask) {
                break;
            }
        }
        let s = |k: &str| a[k].as_str().unwrap_or("").to_string();
        match a["type"].as_str().unwrap_or("") {
            "close-menu" => unsafe {
                if SendMessageW(main, HIDE, 0, 0) == 1 {
                    continue;
                }
                for _ in 0..100 {
                    if IsWindowVisible(main) == 0 {
                        break;
                    }
                    sleep(Duration::from_millis(5));
                }
            },
            "close-submenu" => unsafe {
                SendMessageW(main, PARENT, 0, 0);
            },
            "delay" => sleep(Duration::from_secs_f64(a["duration"].as_f64().unwrap_or(0.0).max(0.0))),
            "execute-command" => {
                let command = expand_command(&s("command"));
                let flags = if is_script(&command) { 0x00000010 } else { 0x08000000 };
                let _ = Command::new("cmd")
                    .arg("/D")
                    .arg("/S")
                    .arg("/C")
                    .raw_arg(format!("\"{}\"", command))
                    .creation_flags(flags)
                    .spawn();
            }
            "open-file" => open_path(&s("path")),
            "open-uri" => open_path(&expand(&s("uri"))),
            "open-menu" => {
                let name = w(&s("menu"));
                let cd = COPYDATASTRUCT { dwData: 1, cbData: (name.len() * 2) as u32, lpData: name.as_ptr() as _ };
                unsafe { SendMessageW(main, WM_COPYDATA, 0, &cd as *const _ as LPARAM) };
            }
            "set-clipboard" => clipboard(&s("text")),
            "focus-window" => focus(&s("appName"), &s("windowName")),
            "simulate-hotkey" => {
                for _ in 0..100 {
                    if unsafe { GetAsyncKeyState(VK_MENU as i32) } >= 0 {
                        break;
                    }
                    sleep(Duration::from_millis(10));
                }
                let codes: Vec<u16> = s("hotkey").split('+').filter_map(scan).collect();
                let mut seq: Vec<(u16, bool, u64)> = codes.iter().map(|&c| (c, true, 10)).collect();
                seq.extend(codes.iter().map(|&c| (c, false, 10)));
                keys(&seq);
            }
            "execute-macro" => {
                let mut seq = vec![];
                for e in a["macro"].as_array().into_iter().flatten() {
                    if let Some(c) = e["key"].as_str().and_then(scan) {
                        seq.push((c, e["type"] == "keyDown", e["delay"].as_u64().unwrap_or(10)));
                    }
                }
                keys(&seq);
            }
            "inhibit-shortcuts" => unsafe {
                SendMessageW(main, INHIBIT, 1, 0);
                inhibited = true;
            },
            "send-websocket-message" => {
                let url = expand(&s("url"));
                let message = expand(&s("message"));
                if let Ok((mut socket, _)) = tungstenite::connect(url.as_str()) {
                    let _ = socket.send(tungstenite::Message::Text(message.into()));
                    let _ = socket.close(None);
                    let _ = socket.flush();
                }
            }
            "open-settings" => {
                let exe = crate::menu::dir().join("orbit-actions-settings.exe");
                if exe.exists() {
                    let _ = Command::new(exe).spawn();
                } else {
                    open_path(&crate::menu::path().to_string_lossy());
                }
            }
            _ => {}
        }
    }
    if inhibited {
        unsafe { SendMessageW(main, INHIBIT, 0, 0) };
    }
}
