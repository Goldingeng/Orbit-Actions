#![windows_subsystem = "windows"]

mod act;
mod apps;
mod draw;
mod icons;
mod keys;
mod math;
mod menu;
mod pie;
#[allow(dead_code)]
mod sound;
#[allow(dead_code)]
mod theme;
mod tr;

use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering::Relaxed};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Input::XboxController::{XInputGetState, XINPUT_STATE};
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use draw::Renderer;
use math::{v, V};
use menu::{Kind, Menu, Settings};
use pie::{Event, Pie};
use serde_json::Value;
use tr::Tr;

pub static MAIN: AtomicIsize = AtomicIsize::new(0);
pub const OWN_INPUT: usize = 0x4F524254;
static SHOWN: AtomicBool = AtomicBool::new(false);
static ALT_HELD: AtomicBool = AtomicBool::new(false);
static ALT_CLICK: AtomicBool = AtomicBool::new(true);

pub const HIDE: u32 = WM_APP + 1;
pub const PARENT: u32 = WM_APP + 2;
const ALT_OPEN: u32 = WM_APP + 3;
const MOVE: u32 = WM_APP + 4;
const DOWN: u32 = WM_APP + 5;
const UP: u32 = WM_APP + 6;
const RIGHT: u32 = WM_APP + 7;
const BACK: u32 = WM_APP + 8;
const KEY: u32 = WM_APP + 9;
const ESCAPE: u32 = WM_APP + 10;
const TRAY: u32 = WM_APP + 11;
pub const INHIBIT: u32 = WM_APP + 12;
const UPDATE: u32 = WM_APP + 14;
const REMOTE: u32 = WM_APP + 15;
const CUSTOM: usize = usize::MAX;
const RELEASES: &str = "https://api.github.com/repos/Goldingeng/Orbit-Actions/releases/latest";

const FRAME_TIMER: usize = 1;
const GESTURE_TIMER: usize = 2;
const PAD_TIMER: usize = 3;
const HOLD_TIMER: usize = 4;

struct Pad {
    buttons: u32,
    stick: (f32, f32),
}

struct App {
    hwnd: HWND,
    button: HWND,
    menus: Vec<Menu>,
    settings: Settings,
    tr: Tr,
    sound: Option<sound::Theme>,
    pie: Option<Pie>,
    renderer: Renderer,
    origin: (i32, i32),
    area: (i32, i32),
    scale: f32,
    dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u8,
    dib: (i32, i32),
    ticking: bool,
    taskbar: u32,
    current: usize,
    opened_at: POINT,
    opened: Instant,
    noop: bool,
    inhibited: bool,
    pad: Pad,
    button_rect: RECT,
    button_hover: bool,
    balloon_url: Option<String>,
    custom: Option<menu::Menu>,
    showing: usize,
    waiting: Option<Vec<usize>>,
}

pub fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn data_dir() -> PathBuf {
    menu::path().parent().map(|p| p.to_path_buf()).unwrap_or_default()
}

fn cursor() -> POINT {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    p
}

fn pack(x: i32, y: i32) -> LPARAM {
    (((x as u32 as u64) << 32) | y as u32 as u64) as LPARAM
}

fn unpack(l: LPARAM) -> (i32, i32) {
    let l = l as u64;
    ((l >> 32) as u32 as i32, l as u32 as i32)
}

fn held(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) < 0 }
}

fn modifiers() -> bool {
    held(VK_CONTROL) || held(VK_SHIFT) || held(VK_MENU) || held(VK_LWIN) || held(VK_RWIN)
}

fn send_text(hwnd: HWND, text: &str) {
    let text = w(text);
    let data = COPYDATASTRUCT { dwData: 1, cbData: (text.len() * 2) as u32, lpData: text.as_ptr() as _ };
    unsafe { SendMessageW(hwnd, WM_COPYDATA, 0, &data as *const _ as LPARAM) };
}

fn register_protocol() {
    let exe = std::env::current_exe().unwrap_or_default();
    let command = w(&format!("\"{}\" \"%1\"", exe.display()));
    let entries: [(&str, &str, Vec<u16>); 3] = [
        ("Software\\Classes\\orbit-actions", "", w("URL:Orbit Actions")),
        ("Software\\Classes\\orbit-actions", "URL Protocol", w("")),
        ("Software\\Classes\\orbit-actions\\shell\\open\\command", "", command),
    ];
    for (key, name, value) in entries {
        let key = w(key);
        let name = w(name);
        unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), if name.len() > 1 { name.as_ptr() } else { null() }, REG_SZ, value.as_ptr() as _, (value.len() * 2) as u32);
        }
    }
}

fn request(args: &[String]) -> Option<String> {
    if let Some(link) = args.iter().find(|a| a.starts_with("orbit-actions://")) {
        return Some(link.clone());
    }
    args.iter().position(|a| a == "--trigger" || a == "--menu").and_then(|i| args.get(i + 1)).cloned().or_else(|| {
        args.iter().any(|a| a == "--settings").then(|| "orbit-actions://settings".to_string())
    })
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(high * 16 + low);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

unsafe fn dark_menus() {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    let lib = LoadLibraryW(w("uxtheme.dll").as_ptr());
    if lib.is_null() {
        return;
    }
    if let Some(allow) = GetProcAddress(lib, 135usize as *const u8) {
        let allow: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(allow);
        allow(1);
    }
    if let Some(flush) = GetProcAddress(lib, 136usize as *const u8) {
        let flush: unsafe extern "system" fn() = std::mem::transmute(flush);
        flush();
    }
}

fn main() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        dark_menus();
        let class = w("orbit-actions");
        let args: Vec<String> = std::env::args().collect();
        let wanted = request(&args);

        let running = FindWindowW(class.as_ptr(), null());
        if !running.is_null() {
            if let Some(text) = wanted {
                let mut pid = 0;
                GetWindowThreadProcessId(running, &mut pid);
                AllowSetForegroundWindow(pid);
                send_text(running, &text);
            }
            return;
        }

        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let button_class = w("orbit-actions-button");
        let wc = WNDCLASSW { lpfnWndProc: Some(DefWindowProcW), hInstance: instance, lpszClassName: button_class.as_ptr(), ..std::mem::zeroed() };
        RegisterClassW(&wc);

        let style = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT;
        let hwnd = CreateWindowExW(style, class.as_ptr(), class.as_ptr(), WS_POPUP, 0, 0, 1, 1, null_mut(), null_mut(), instance, null());
        let button = CreateWindowExW(style, button_class.as_ptr(), button_class.as_ptr(), WS_POPUP, 0, 0, 1, 1, null_mut(), null_mut(), instance, null());
        MAIN.store(hwnd as isize, Relaxed);

        let screen = GetDC(null_mut());
        let dc = CreateCompatibleDC(screen);
        ReleaseDC(null_mut(), screen);

        let settings = menu::settings();
        ALT_CLICK.store(settings.alt_click, Relaxed);
        let app = Box::into_raw(Box::new(App {
            hwnd,
            button,
            menus: menu::load(),
            tr: Tr::new(&settings.locale),
            sound: sound::Theme::load(&settings.sound_theme, settings.volume),
            renderer: Renderer::new(settings.theme.clone(), 1.0, settings.animations),
            settings,
            pie: None,
            origin: (0, 0),
            area: (1, 1),
            scale: 1.0,
            dc,
            bitmap: null_mut(),
            bits: null_mut(),
            dib: (0, 0),
            ticking: false,
            taskbar: RegisterWindowMessageW(w("TaskbarCreated").as_ptr()),
            current: 0,
            opened_at: POINT { x: 0, y: 0 },
            opened: Instant::now(),
            noop: true,
            inhibited: false,
            pad: Pad { buttons: 0, stick: (0.0, 0.0) },
            button_rect: RECT { left: 0, top: 0, right: 0, bottom: 0 },
            button_hover: false,
            balloon_url: None,
            custom: None,
            showing: 0,
            waiting: None,
        }));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, app as _);

        register_protocol();
        hotkeys(&*app);
        tray(&*app, NIM_ADD);
        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), instance, 0);
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), instance, 0);
        std::thread::spawn(draw::warm);
        if (*app).settings.version_check {
            check_version(hwnd as isize);
        }
        listen(hwnd as isize);

        if let Some(text) = wanted {
            dispatch(&mut *app, &text);
        }

        let mut msg = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe fn hotkeys(app: &App) {
    for i in 0..256 {
        UnregisterHotKey(app.hwnd, i);
    }
    if app.inhibited {
        return;
    }
    let mut seen: Vec<&str> = vec![];
    for (i, m) in app.menus.iter().enumerate() {
        if m.shortcut.is_empty() || seen.contains(&m.shortcut.as_str()) {
            continue;
        }
        seen.push(&m.shortcut);
        if let Some((mods, key)) = act::shortcut(&m.shortcut) {
            RegisterHotKey(app.hwnd, i as i32, mods, key);
        }
    }
}

fn text_matches(rule: &Value, value: &str) -> bool {
    let rule = rule.as_str().unwrap_or("");
    if rule.is_empty() {
        return true;
    }
    let pattern = match rule.strip_prefix('/').and_then(|r| r.rsplit_once('/')) {
        Some((body, _)) => body,
        None => rule,
    };
    let value = value.to_lowercase();
    pattern.to_lowercase().split('|').any(|part| value.contains(part))
}

fn conditions_met(menu: &Menu, app_name: &str, window: &str, p: POINT) -> bool {
    let c = &menu.conditions;
    if !c.is_object() {
        return true;
    }
    if !text_matches(&c["appName"], app_name) || !text_matches(&c["windowName"], window) {
        return false;
    }
    let area = &c["screenArea"];
    let x = p.x as f64;
    let y = p.y as f64;
    area["xMin"].as_f64().is_none_or(|v| x >= v)
        && area["xMax"].as_f64().is_none_or(|v| x <= v)
        && area["yMin"].as_f64().is_none_or(|v| y >= v)
        && area["yMax"].as_f64().is_none_or(|v| y <= v)
}

fn has_rules(menu: &Menu) -> bool {
    let c = &menu.conditions;
    c["appName"].as_str().is_some_and(|s| !s.is_empty())
        || c["windowName"].as_str().is_some_and(|s| !s.is_empty())
        || c["screenArea"].as_object().is_some_and(|o| o.values().any(|v| v.is_number()))
}

unsafe fn hotkey(app: &mut App, id: usize) {
    let Some(shortcut) = app.menus.get(id).map(|m| m.shortcut.clone()) else { return };
    let group: Vec<usize> = (0..app.menus.len()).filter(|&i| app.menus[i].shortcut == shortcut).collect();
    let shown = app.pie.as_ref().is_some_and(|p| !p.hidden);

    if shown {
        let next = match app.settings.same_shortcut.as_str() {
            "close" => None,
            "cycle-from-first" | "cycle-from-recent" => {
                let pos = group.iter().position(|&i| i == app.current).unwrap_or(0);
                Some(group[(pos + 1) % group.len()])
            }
            _ => return,
        };
        if let Some(pie) = app.pie.as_mut() {
            pie.hide();
        }
        handle_events(app);
        match next {
            Some(n) => {
                finish_hide(app);
                open(app, n);
            }
            None => frame(app),
        }
        return;
    }

    let p = cursor();
    let fg = GetForegroundWindow();
    let mut pid = 0;
    GetWindowThreadProcessId(fg, &mut pid);
    let app_name = act::exe_name(pid);
    let window = act::title(fg);
    let matching: Vec<usize> = group.iter().copied().filter(|&i| conditions_met(&app.menus[i], &app_name, &window, p)).collect();
    let choice = if app.settings.same_shortcut == "cycle-from-recent" && matching.contains(&app.current) {
        Some(app.current)
    } else {
        matching.iter().copied().find(|&i| has_rules(&app.menus[i])).or(matching.first().copied())
    };
    if let Some(i) = choice {
        open(app, i);
    }
}

unsafe fn star_icon(color: &str, size: i32) -> Option<HICON> {
    let source = include_str!("../assets/brand/star-tray.svg").replace("#fff", color);
    let pm = draw::svg(&source, size as f32, None)?;
    let (width, height) = (pm.width() as i32, pm.height() as i32);
    let mut bgra = pm.data().to_vec();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let row = ((width + 15) / 16 * 2) as usize;
    let mask = vec![0u8; row * height as usize];
    let icon = CreateIcon(GetModuleHandleW(null()), width, height, 1, 32, mask.as_ptr(), bgra.as_ptr());
    (!icon.is_null()).then_some(icon)
}

unsafe fn tray_data(app: &App) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = app.hwnd;
    nid.uID = 1;
    nid
}

unsafe fn tray(app: &App, op: NOTIFY_ICON_MESSAGE) {
    let flavor = app.settings.tray.as_str();
    if flavor == "none" || flavor == "hidden" {
        return;
    }
    let mut nid = tray_data(app);
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = TRAY;
    let size = GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForWindow(app.hwnd)).max(16);
    let color = if flavor == "black" { "#000" } else { "#fff" };
    let own = star_icon(color, size);
    nid.hIcon = own.unwrap_or_else(|| LoadIconW(null_mut(), IDI_APPLICATION));
    for (i, c) in "Orbit Actions".encode_utf16().enumerate() {
        nid.szTip[i] = c;
    }
    Shell_NotifyIconW(op, &nid);
    if let Some(icon) = own {
        DestroyIcon(icon);
    }
}

unsafe fn balloon(app: &App, title: &str, text: &str) {
    let mut nid = tray_data(app);
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON;
    for (i, c) in title.encode_utf16().take(63).enumerate() {
        nid.szInfoTitle[i] = c;
    }
    for (i, c) in text.encode_utf16().take(255).enumerate() {
        nid.szInfo[i] = c;
    }
    Shell_NotifyIconW(NIM_MODIFY, &nid);
}

fn http_get(url: &str) -> Option<String> {
    use windows_sys::Win32::Networking::WinInet::*;
    unsafe {
        let agent = w("Orbit-Actions");
        let session = InternetOpenW(agent.as_ptr(), INTERNET_OPEN_TYPE_PRECONFIG, null(), null(), 0);
        if session.is_null() {
            return None;
        }
        let address = w(url);
        let headers = w("Accept: application/vnd.github+json\r\n");
        let request = InternetOpenUrlW(session, address.as_ptr(), headers.as_ptr(), u32::MAX, INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE, 0);
        if request.is_null() {
            InternetCloseHandle(session);
            return None;
        }
        let mut status = 0u32;
        let mut size = 4u32;
        HttpQueryInfoW(request, HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER, &mut status as *mut u32 as _, &mut size, null_mut());
        let mut body = vec![];
        let mut chunk = [0u8; 8192];
        loop {
            let mut read = 0u32;
            if InternetReadFile(request, chunk.as_mut_ptr() as _, chunk.len() as u32, &mut read) == 0 || read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read as usize]);
        }
        InternetCloseHandle(request);
        InternetCloseHandle(session);
        (status == 200).then(|| String::from_utf8_lossy(&body).to_string())
    }
}

fn version_numbers(v: &str) -> Vec<u32> {
    v.trim().trim_start_matches(['v', 'V']).split(['.', '-']).map_while(|p| p.parse().ok()).collect()
}

static INBOX: std::sync::Mutex<Vec<(u32, String)>> = std::sync::Mutex::new(Vec::new());

fn post_text(hwnd: isize, msg: u32, text: String) {
    let mut inbox = INBOX.lock().unwrap();
    if inbox.len() >= 256 {
        return;
    }
    if unsafe { PostMessageW(hwnd as HWND, msg, 0, 0) } != 0 {
        inbox.push((msg, text));
    }
}

fn take_texts(msg: u32) -> Vec<String> {
    let mut inbox = INBOX.lock().unwrap();
    let (taken, kept): (Vec<_>, Vec<_>) = inbox.drain(..).partition(|(kind, _)| *kind == msg);
    *inbox = kept;
    taken.into_iter().map(|(_, text)| text).collect()
}

fn listen(hwnd: isize) {
    let Ok(listener) = std::net::TcpListener::bind("127.0.0.1:0") else { return };
    let Ok(port) = listener.local_addr().map(|a| a.port()) else { return };
    if let Ok(appdata) = std::env::var("APPDATA") {
        let dir = PathBuf::from(appdata).join("kando");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("ipc-info.json"), format!("{{\n  \"port\": {port},\n  \"apiVersion\": 2\n}}"));
    }
    static CLIENTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if CLIENTS.fetch_add(1, Relaxed) >= 16 {
                CLIENTS.fetch_sub(1, Relaxed);
                continue;
            }
            std::thread::spawn(move || {
                struct Leave;
                impl Drop for Leave {
                    fn drop(&mut self) {
                        CLIENTS.fetch_sub(1, Relaxed);
                    }
                }
                let _leave = Leave;
                let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                let from_page = |request: &tungstenite::handshake::server::Request, response| {
                    if request.headers().contains_key("origin") {
                        let mut refusal = tungstenite::handshake::server::ErrorResponse::new(None);
                        *refusal.status_mut() = tungstenite::http::StatusCode::FORBIDDEN;
                        return Err(refusal);
                    }
                    Ok(response)
                };
                let mut limits = tungstenite::protocol::WebSocketConfig::default();
                limits.max_message_size = Some(1 << 20);
                limits.max_frame_size = Some(1 << 20);
                let Ok(mut socket) = tungstenite::accept_hdr_with_config(stream, from_page, Some(limits)) else { return };
                while let Ok(message) = socket.read() {
                    if let tungstenite::Message::Text(text) = message {
                        post_text(hwnd, REMOTE, text.to_string());
                    }
                }
            });
        }
    });
}

unsafe fn remote(app: &mut App, text: &str) {
    let message: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    match message["type"].as_str().unwrap_or("") {
        "show-menu" => {
            let name = message["name"].as_str().unwrap_or("");
            open_named(app, name);
        }
        "show-custom-menu" if message["menu"].is_object() => {
            let custom = menu::custom(&message["menu"]);
            KillTimer(app.hwnd, HOLD_TIMER);
            let waiting = app.waiting.take();
            let open_now = app.pie.as_ref().filter(|p| !p.hidden);
            if let (Some(path), Some(pie)) = (waiting, open_now) {
                if pie.center_path() == path {
                    graft(app, &path, custom);
                    return;
                }
            }
            if app.pie.as_ref().is_some_and(|p| !p.hidden) {
                finish_hide(app);
            }
            app.custom = Some(custom);
            open(app, CUSTOM);
        }
        _ => {}
    }
}

unsafe fn graft(app: &mut App, path: &[usize], custom: menu::Menu) {
    let source = if app.showing == CUSTOM { app.custom.as_ref() } else { app.menus.get(app.showing) };
    let Some(source) = source else { return };
    let mut root = source.root.clone();
    let mut node = &mut root;
    for &i in path {
        node = &mut node.children[i];
    }
    node.children = custom.root.children;
    node.name = custom.root.name;
    node.icon = custom.root.icon;
    node.theme = custom.root.theme;
    node.img = custom.root.img;
    let (fixed, anchored, hover_mode) = (source.fixed, source.anchored, source.hover_mode);
    let Some(old) = app.pie.as_ref() else { return };
    let chain = old.chain(path);
    let (size, mouse) = (old.size, old.input.abs);
    let mut pie = Pie::new(&root, &app.settings, size, mouse, fixed, anchored, hover_mode);
    pie.max_radius = app.settings.theme.max_radius;
    pie.restore(path, &chain);
    app.custom = Some(menu::Menu { root, shortcut: String::new(), fixed, anchored, hover_mode, conditions: Value::Null });
    app.showing = CUSTOM;
    app.pie = Some(pie);
    handle_events(app);
    frame(app);
}

fn check_version(hwnd: isize) {
    std::thread::spawn(move || {
        let Some(body) = http_get(RELEASES) else { return };
        let release: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        let Some(tag) = release["tag_name"].as_str() else { return };
        if version_numbers(tag) > version_numbers(env!("CARGO_PKG_VERSION")) {
            let url = release["html_url"].as_str().unwrap_or("https://github.com/Goldingeng/Orbit-Actions/releases/latest").to_string();
            post_text(hwnd, UPDATE, url);
        }
    });
}

fn open_settings() {
    open_editor(&[]);
}

fn open_editor(args: &[&str]) {
    let exe = std::env::current_exe().unwrap_or_default().with_file_name("orbit-actions-settings.exe");
    if exe.exists() {
        let _ = std::process::Command::new(exe).args(args).spawn();
    } else {
        act::open_path(&menu::path().to_string_lossy());
    }
}

unsafe fn tray_menu(app: &mut App) {
    let m = CreatePopupMenu();
    for (i, menu) in app.menus.iter().enumerate().take(50) {
        let label = if menu.shortcut.is_empty() { menu.root.name.clone() } else { format!("{} ({})", menu.root.name, menu.shortcut) };
        AppendMenuW(m, MF_STRING, 100 + i, w(&label).as_ptr());
    }
    if !app.menus.is_empty() {
        AppendMenuW(m, MF_SEPARATOR, 0, null());
    }
    AppendMenuW(m, MF_STRING, 1, w(&app.tr.t("main.show-settings")).as_ptr());
    let inhibit = if app.inhibited { app.tr.t("main.un-inhibit-shortcuts") } else { app.tr.t("main.inhibit-shortcuts") };
    AppendMenuW(m, MF_STRING, 2, w(&inhibit).as_ptr());
    AppendMenuW(m, MF_SEPARATOR, 0, null());
    AppendMenuW(m, MF_STRING, 3, w(&app.tr.t("main.quit")).as_ptr());
    let p = cursor();
    SetForegroundWindow(app.hwnd);
    let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, p.x, p.y, 0, app.hwnd, null());
    DestroyMenu(m);

    match cmd {
        1 => open_settings(),
        2 => {
            app.inhibited = !app.inhibited;
            ALT_CLICK.store(app.settings.alt_click && !app.inhibited, Relaxed);
            hotkeys(app);
        }
        3 => {
            let nid = tray_data(app);
            Shell_NotifyIconW(NIM_DELETE, &nid);
            DestroyWindow(app.hwnd);
        }
        c if c >= 100 => open(app, (c - 100) as usize),
        _ => {}
    }
}

unsafe fn reload(app: &mut App) {
    app.menus = menu::load();
    app.settings = menu::settings();
    app.tr = Tr::new(&app.settings.locale);
    app.sound = sound::Theme::load(&app.settings.sound_theme, app.settings.volume);
    icons::clear();
    ALT_CLICK.store(app.settings.alt_click && !app.inhibited, Relaxed);
    hotkeys(app);
    let nid = tray_data(app);
    Shell_NotifyIconW(NIM_DELETE, &nid);
    tray(app, NIM_ADD);
}

unsafe fn dispatch(app: &mut App, text: &str) {
    if text == "--reload" {
        reload(app);
        return;
    }
    if let Some(rest) = text.strip_prefix("orbit-actions://") {
        let (command, query) = rest.trim_end_matches('/').split_once('?').unwrap_or((rest.trim_end_matches('/'), ""));
        match command {
            "menu" => {
                let name = query.split('&').find_map(|kv| kv.strip_prefix("name=")).map(decode).unwrap_or_default();
                open_named(app, &name);
            }
            "settings" => open_settings(),
            _ => {
                let title = app.tr.t("main.invalid-link-header");
                let body = app.tr.t("main.invalid-link-message");
                balloon(app, &title, &body);
            }
        }
        return;
    }
    if app.pie.as_ref().is_some_and(|p| !p.hidden) {
        if let Some(pie) = app.pie.as_mut() {
            pie.hide();
        }
        handle_events(app);
        finish_hide(app);
    }
    open_named(app, text);
}

unsafe extern "system" fn mouse_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lp as *const MSLLHOOKSTRUCT);
        let hwnd = MAIN.load(Relaxed) as HWND;
        let at = pack(info.pt.x, info.pt.y);
        let msg = wp as u32;
        if SHOWN.load(Relaxed) {
            match msg {
                WM_MOUSEMOVE => {
                    PostMessageW(hwnd, MOVE, 0, at);
                }
                WM_LBUTTONDOWN | WM_MBUTTONDOWN => {
                    PostMessageW(hwnd, DOWN, 0, at);
                    return 1;
                }
                WM_LBUTTONUP | WM_MBUTTONUP => {
                    ALT_HELD.store(false, Relaxed);
                    PostMessageW(hwnd, UP, 0, at);
                    return 1;
                }
                WM_RBUTTONDOWN => {
                    PostMessageW(hwnd, RIGHT, 0, at);
                    return 1;
                }
                WM_XBUTTONDOWN => {
                    if (info.mouseData >> 16) as u16 == XBUTTON1 {
                        PostMessageW(hwnd, BACK, 0, at);
                    } else {
                        PostMessageW(hwnd, DOWN, 0, at);
                    }
                    return 1;
                }
                WM_RBUTTONUP | WM_XBUTTONUP => return 1,
                _ => {}
            }
        } else if ALT_CLICK.load(Relaxed) {
            if msg == WM_LBUTTONDOWN && held(VK_MENU) {
                ALT_HELD.store(true, Relaxed);
                PostMessageW(hwnd, ALT_OPEN, 0, at);
                return 1;
            }
            if msg == WM_LBUTTONUP && ALT_HELD.swap(false, Relaxed) {
                PostMessageW(hwnd, UP, 0, at);
                return 1;
            }
        }
    }
    CallNextHookEx(null_mut(), code, wp, lp)
}

fn is_modifier(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

unsafe extern "system" fn keyboard_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 && SHOWN.load(Relaxed) {
        let info = &*(lp as *const KBDLLHOOKSTRUCT);
        if info.dwExtraInfo != OWN_INPUT {
            let hwnd = MAIN.load(Relaxed) as HWND;
            let down = matches!(wp as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let vk = info.vkCode;
            if is_modifier(vk) {
                PostMessageW(hwnd, KEY, vk as usize, down as isize);
            } else if vk == VK_ESCAPE as u32 {
                if down {
                    PostMessageW(hwnd, ESCAPE, 0, 0);
                }
                return 1;
            } else if !modifiers() && !key_name(vk).is_empty() {
                PostMessageW(hwnd, KEY, vk as usize, down as isize);
                return 1;
            }
        }
    }
    CallNextHookEx(null_mut(), code, wp, lp)
}

fn key_name(vk: u32) -> String {
    match vk {
        0x41..=0x5A => ((vk as u8) as char).to_ascii_lowercase().to_string(),
        0x30..=0x39 => ((vk as u8) as char).to_string(),
        0x60..=0x69 => format!("num{}", vk - 0x60),
        0x70..=0x87 => format!("f{}", vk - 0x6F),
        0x08 => "backspace".into(),
        0x09 => "tab".into(),
        0x0D => "enter".into(),
        0x20 => "space".into(),
        0x25 => "arrowleft".into(),
        0x26 => "arrowup".into(),
        0x27 => "arrowright".into(),
        0x28 => "arrowdown".into(),
        0x2E => "delete".into(),
        0x24 => "home".into(),
        0x23 => "end".into(),
        0x21 => "pageup".into(),
        0x22 => "pagedown".into(),
        _ => String::new(),
    }
}

unsafe fn to_local(app: &App, x: i32, y: i32) -> V {
    v((x - app.origin.0) as f32 / app.scale, (y - app.origin.1) as f32 / app.scale)
}

unsafe fn open_named(app: &mut App, name: &str) {
    let found = app.menus.iter().position(|m| m.shortcut == name || m.root.name == name);
    if let Some(i) = found {
        open(app, i);
    }
}

unsafe fn open(app: &mut App, index: usize) {
    if app.pie.as_ref().is_some_and(|p| !p.hidden) {
        return;
    }
    let found = if index == CUSTOM { app.custom.as_ref() } else { app.menus.get(index) };
    let Some(menu) = found else { return };
    if index != CUSTOM {
        app.current = index;
    }
    app.showing = index;
    app.waiting = None;

    let p = cursor();
    app.opened_at = p;
    app.opened = Instant::now();
    app.noop = true;
    let fg = GetForegroundWindow();
    let mut pid = 0;
    GetWindowThreadProcessId(fg, &mut pid);
    *act::WM_INFO.lock().unwrap() = (act::exe_name(pid), act::title(fg), p.x, p.y);

    let monitor = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(monitor, &mut info);
    let area = info.rcWork;
    let (mut dpi_x, mut dpi_y) = (96, 96);
    GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);

    app.origin = (area.left, area.top);
    app.area = (area.right - area.left, area.bottom - area.top);
    app.scale = dpi_x as f32 / 96.0 * app.settings.zoom;
    let size = v(app.area.0 as f32 / app.scale, app.area.1 as f32 / app.scale);
    let mouse = to_local(app, p.x, p.y);

    app.settings.theme = theme::load(&menu::config());
    app.renderer = Renderer::new(app.settings.theme.clone(), app.scale, app.settings.animations);
    app.renderer.origin = app.origin;
    app.renderer.area = app.area;
    app.renderer.wedges = app.settings.wedges;
    app.renderer.quick_keys = app.settings.quick_keys;
    app.renderer.underline = app.settings.underline;
    app.renderer.reactive = app.settings.reactive;
    app.renderer.fade_to(1.0, app.settings.fade_in);

    let mut pie = Pie::new(&menu.root, &app.settings, size, mouse, menu.fixed, menu.anchored, menu.hover_mode);
    pie.max_radius = app.settings.theme.max_radius;
    app.pie = Some(pie);
    SHOWN.store(true, Relaxed);
    timeBeginPeriod(1);
    handle_events(app);
    frame(app);
    ShowWindow(app.hwnd, SW_SHOWNOACTIVATE);
    show_button(app);
    if app.settings.gamepad {
        app.pad = Pad { buttons: pad_state().map(|s| s.0).unwrap_or(0), stick: (0.0, 0.0) };
        SetTimer(app.hwnd, PAD_TIMER, 16, None);
    }
}

unsafe fn finish_hide(app: &mut App) {
    ShowWindow(app.hwnd, SW_HIDE);
    ShowWindow(app.button, SW_HIDE);
    KillTimer(app.hwnd, FRAME_TIMER);
    KillTimer(app.hwnd, GESTURE_TIMER);
    KillTimer(app.hwnd, PAD_TIMER);
    app.ticking = false;
    app.pie = None;
    timeEndPeriod(1);
}

fn workflow(pie: &Pie, path: &[usize], pick: fn(&menu::Item) -> &Vec<Value>) -> Option<(Vec<Value>, Kind, bool)> {
    let node = pie.nodes.iter().find(|n| n.path == path)?;
    let actions = pick(&node.item);
    if actions.is_empty() {
        return None;
    }
    Some((actions.clone(), node.item.kind, node.item.remote))
}

fn is_noop(actions: &[Value]) -> bool {
    actions.iter().all(|a| matches!(a["type"].as_str().unwrap_or(""), "delay" | "inhibit-shortcuts" | "close-menu" | "close-submenu"))
}

unsafe fn handle_events(app: &mut App) {
    loop {
        let Some(pie) = app.pie.as_mut() else { return };
        if pie.events.is_empty() {
            break;
        }
        let events: Vec<Event> = pie.events.drain(..).collect();
        for event in events {
            let Some(pie) = app.pie.as_ref() else { return };
            let job = match &event {
                Event::Hide => {
                    SHOWN.store(false, Relaxed);
                    ShowWindow(app.button, SW_HIDE);
                    app.renderer.fade_to(0.0, app.settings.fade_out);
                    None
                }
                Event::Sound(kind) => {
                    if let Some(s) = &app.sound {
                        s.play(kind);
                    }
                    None
                }
                Event::Warp(target) => {
                    let p = cursor();
                    let x = app.origin.0 + (target.x * app.scale).round() as i32;
                    let y = app.origin.1 + (target.y * app.scale).round() as i32;
                    if p.x != x || p.y != y {
                        SetCursorPos(x, y);
                    }
                    None
                }
                Event::Select(path) => {
                    if app.settings.return_pointer {
                        SetCursorPos(app.opened_at.x, app.opened_at.y);
                    }
                    workflow(pie, path, |i| &i.select).filter(|w| w.1 == Kind::Button)
                }
                Event::Open(path) => workflow(pie, path, |i| &i.open).filter(|w| w.1 == Kind::Submenu),
                Event::Activate(path) => workflow(pie, path, |i| &i.activate),
                Event::Hover(path) => workflow(pie, path, |i| &i.hover),
            };
            if let Some((actions, _, remote)) = job {
                if !is_noop(&actions) {
                    app.noop = false;
                }
                if let Event::Open(path) = &event {
                    let kinds: Vec<&str> = actions.iter().map(|a| a["type"].as_str().unwrap_or("")).collect();
                    let close = kinds.iter().position(|k| *k == "close-menu");
                    let command = kinds.iter().rposition(|k| *k == "execute-command");
                    if let (Some(close), Some(command)) = (close, command) {
                        if close < command {
                            app.waiting = Some(path.clone());
                            SetTimer(app.hwnd, HOLD_TIMER, 1500, None);
                        }
                    }
                }
                let ask = remote.then(|| (app.tr.t("main.ipc-menu-warning-title"), app.tr.t("main.ipc-menu-warning-message")));
                std::thread::spawn(move || act::run(&actions, ask.as_ref()));
            }
        }
    }
}

unsafe fn frame(app: &mut App) {
    let Some(pie) = app.pie.as_ref() else { return };
    let Some(f) = app.renderer.render(pie) else { return };
    present(app, &f);
    app.renderer.recycle(f.pixmap);

    let Some(pie) = app.pie.as_ref() else { return };
    if app.renderer.faded_out() && pie.hidden {
        finish_hide(app);
        return;
    }
    let busy = app.renderer.busy;
    if busy && !app.ticking {
        SetTimer(app.hwnd, FRAME_TIMER, 8, None);
        app.ticking = true;
    } else if !busy && app.ticking {
        KillTimer(app.hwnd, FRAME_TIMER);
        app.ticking = false;
    }
    if let Some(deadline) = pie.gesture.deadline {
        let ms = deadline.saturating_duration_since(Instant::now()).as_millis() as u32;
        SetTimer(app.hwnd, GESTURE_TIMER, ms.max(1), None);
    }
}

unsafe fn blit_window(app: &mut App, hwnd: HWND, pm: &tiny_skia::Pixmap, x: i32, y: i32, alpha: f32) {
    let w = pm.width() as i32;
    let h = pm.height() as i32;
    if w > app.dib.0 || h > app.dib.1 {
        let dw = w.max(app.dib.0);
        let dh = h.max(app.dib.1);
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = dw;
        bi.bmiHeader.biHeight = -dh;
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        let mut bits = null_mut();
        let bmp = CreateDIBSection(app.dc, &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if bmp.is_null() || bits.is_null() {
            return;
        }
        SelectObject(app.dc, bmp);
        if !app.bitmap.is_null() {
            DeleteObject(app.bitmap);
        }
        app.bitmap = bmp;
        app.bits = bits as *mut u8;
        app.dib = (dw, dh);
    }

    let stride = app.dib.0 as usize * 4;
    let src = pm.data();
    for row in 0..h as usize {
        let dst = std::slice::from_raw_parts_mut(app.bits.add(row * stride), w as usize * 4);
        let from = &src[row * w as usize * 4..(row + 1) * w as usize * 4];
        for (d, s) in dst.chunks_exact_mut(4).zip(from.chunks_exact(4)) {
            d[0] = s[2];
            d[1] = s[1];
            d[2] = s[0];
            d[3] = s[3];
        }
    }

    let pos = POINT { x, y };
    let size = SIZE { cx: w, cy: h };
    let zero = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: (alpha.clamp(0.0, 1.0) * 255.0) as u8,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    UpdateLayeredWindow(hwnd, null_mut(), &pos, &size, app.dc, &zero, 0, &blend, ULW_ALPHA);
}

unsafe fn present(app: &mut App, f: &draw::Frame) {
    blit_window(app, app.hwnd, &f.pixmap, f.x, f.y, f.alpha);
}

unsafe fn show_button(app: &mut App) {
    let s = app.scale;
    let size = (50.0 * s) as i32;
    let margin = (10.0 * s) as i32;
    let (l, t) = app.origin;
    let (w, h) = app.area;
    let pos = app.settings.button_position.as_str();
    let x = if pos.ends_with("left") { l + margin } else { l + w - margin - size };
    let y = if pos.starts_with("top") { t + margin } else { t + h - margin - size };
    app.button_rect = RECT { left: x, top: y, right: x + size, bottom: y + size };
    app.button_hover = false;
    draw_button(app);
    ShowWindow(app.button, SW_SHOWNOACTIVATE);
}

unsafe fn draw_button(app: &mut App) {
    let r = app.button_rect;
    let size = (r.right - r.left).max(1) as u32;
    let Some(mut pm) = tiny_skia::Pixmap::new(size, size) else { return };
    let s = size as f32;
    let corner = |big: bool| if big { s * 0.4 } else { s * 0.2 };
    let pos = app.settings.button_position.clone();
    let (tl, tr, br, bl) = match pos.as_str() {
        "top-left" => (false, false, true, false),
        "top-right" => (false, false, false, true),
        "bottom-left" => (false, true, false, false),
        _ => (true, false, false, false),
    };
    let mut pb = tiny_skia::PathBuilder::new();
    let (a, b, c, d) = (corner(tl), corner(tr), corner(br), corner(bl));
    pb.move_to(a, 0.0);
    pb.line_to(s - b, 0.0);
    pb.quad_to(s, 0.0, s, b);
    pb.line_to(s, s - c);
    pb.quad_to(s, s, s - c, s);
    pb.line_to(d, s);
    pb.quad_to(0.0, s, 0.0, s - d);
    pb.line_to(0.0, a);
    pb.quad_to(0.0, 0.0, a, 0.0);
    pb.close();
    let fill = if app.button_hover { 0.8 } else { 0.5 };
    if let Some(path) = pb.finish() {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(0, 0, 0, (fill * 255.0) as u8);
        paint.anti_alias = true;
        pm.fill_path(&path, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
    }
    let gear = menu::Item {
        name: String::new(),
        icon: "settings".into(),
        theme: "material-symbols-rounded".into(),
        img: None,
        kind: Kind::Button,
        angle: None,
        select: vec![],
        open: vec![],
        activate: vec![],
        hover: vec![],
        key: None,
        activate_key: None,
        children: vec![],
        remote: false,
    };
    draw::draw_icon(&mut pm, &gear, v(s / 2.0, s / 2.0), s * 0.62, [1.0, 1.0, 1.0, 1.0], 1.0, false);
    let alpha = if app.settings.hide_button && !app.button_hover { 0.0 } else { 1.0 };
    let button = app.button;
    blit_window(app, button, &pm, r.left, r.top, alpha);
}

fn in_rect(r: &RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

fn pad_state() -> Option<(u32, (f32, f32), (f32, f32))> {
    for i in 0..4 {
        let mut state: XINPUT_STATE = unsafe { std::mem::zeroed() };
        if unsafe { XInputGetState(i, &mut state) } == 0 {
            let g = state.Gamepad;
            let b = g.wButtons as u32;
            let map = [
                (0x1000, 0),
                (0x2000, 1),
                (0x4000, 2),
                (0x8000, 3),
                (0x0100, 4),
                (0x0200, 5),
                (0x0020, 8),
                (0x0010, 9),
                (0x0040, 10),
                (0x0080, 11),
                (0x0001, 12),
                (0x0002, 13),
                (0x0004, 14),
                (0x0008, 15),
            ];
            let mut web = 0u32;
            for (mask, index) in map {
                if b & mask != 0 {
                    web |= 1 << index;
                }
            }
            if g.bLeftTrigger > 128 {
                web |= 1 << 6;
            }
            if g.bRightTrigger > 128 {
                web |= 1 << 7;
            }
            let n = |v: i16| (v as f32 / 32767.0).clamp(-1.0, 1.0);
            return Some((web, (n(g.sThumbLX), -n(g.sThumbLY)), (n(g.sThumbRX), -n(g.sThumbRY))));
        }
    }
    None
}

unsafe fn poll_pad(app: &mut App) {
    let Some((buttons, left, right)) = pad_state() else { return };
    let Some(pie) = app.pie.as_mut().filter(|p| !p.hidden) else { return };
    let axes = [left.0, left.1, right.0, right.1];
    let mut best = 0;
    for i in 0..4 {
        if axes[i].abs() > axes[best].abs() {
            best = i;
        }
    }
    let (x, y) = if best >= 2 { right } else { left };
    let tilt = (x * x + y * y).sqrt();
    let stick = if tilt > 0.3 { (x, y) } else { (0.0, 0.0) };
    let mut changed = false;
    if stick != app.pad.stick {
        app.pad.stick = stick;
        pie.stick(stick.0, stick.1);
        changed = true;
    }
    let pressed = buttons & !app.pad.buttons;
    app.pad.buttons = buttons;
    for index in 0..16 {
        if pressed & (1 << index) != 0 {
            pie.pad_button(index, app.settings.back_button, app.settings.close_button);
            changed = true;
        }
    }
    if changed {
        handle_events(app);
        frame(app);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let app = &mut *ptr;

    if msg == app.taskbar && msg != 0 {
        tray(app, NIM_ADD);
        return 0;
    }

    match msg {
        WM_HOTKEY => hotkey(app, wp),
        ALT_OPEN => {
            act::mask_alt();
            let index = app.menus.iter().position(|m| m.shortcut.eq_ignore_ascii_case("Control+Space")).unwrap_or(0);
            open(app, index);
        }
        MOVE | DOWN | UP | RIGHT | BACK => {
            let mut lp = lp;
            if msg == MOVE {
                let mut next: MSG = std::mem::zeroed();
                while PeekMessageW(&mut next, hwnd, MOVE, MOVE, PM_REMOVE) != 0 {
                    lp = next.lParam;
                }
            }
            let (x, y) = unpack(lp);
            let over_button = SHOWN.load(Relaxed) && in_rect(&app.button_rect, x, y);
            if msg == MOVE && over_button != app.button_hover {
                app.button_hover = over_button;
                draw_button(app);
            }
            if msg == DOWN && over_button {
                if let Some(pie) = app.pie.as_mut() {
                    pie.hide();
                }
                handle_events(app);
                frame(app);
                open_settings();
                return 0;
            }
            let at = to_local(app, x, y);
            if let Some(pie) = app.pie.as_mut().filter(|p| !p.hidden) {
                match msg {
                    MOVE => pie.motion(at, modifiers()),
                    DOWN => pie.press(at),
                    UP => pie.release(at),
                    RIGHT => pie.right_click(),
                    _ => pie.back_button(),
                }
                handle_events(app);
                frame(app);
            }
        }
        KEY => {
            let vk = wp as u32;
            let down = lp != 0;
            if let Some(pie) = app.pie.as_mut().filter(|p| !p.hidden) {
                if is_modifier(vk) {
                    if down {
                        pie.modifier_down();
                    } else {
                        pie.modifier_up(modifiers());
                    }
                } else if !modifiers() {
                    let name = key_name(vk);
                    if !name.is_empty() && !pie.key(&name, down) && down {
                        pie.modifier_down();
                    }
                }
                handle_events(app);
                frame(app);
            }
        }
        ESCAPE | HIDE => {
            let held = app.waiting.as_ref().is_some_and(|w| app.pie.as_ref().is_some_and(|p| !p.hidden && p.center_path() == *w));
            if msg == HIDE && held {
                return 1;
            }
            app.waiting = None;
            KillTimer(hwnd, HOLD_TIMER);
            if let Some(pie) = app.pie.as_mut() {
                pie.hide();
                handle_events(app);
                frame(app);
            }
        }
        PARENT => {
            if let Some(pie) = app.pie.as_mut().filter(|p| !p.hidden) {
                pie.close_submenu();
                handle_events(app);
                frame(app);
            }
        }
        INHIBIT => {
            app.inhibited = wp != 0;
            ALT_CLICK.store(app.settings.alt_click && !app.inhibited, Relaxed);
            hotkeys(app);
        }
        REMOTE => {
            for text in take_texts(REMOTE) {
                remote(app, &text);
            }
        }
        UPDATE => {
            if let Some(url) = take_texts(UPDATE).pop() {
                let title = app.tr.t("main.new-version-notification-header");
                let body = app.tr.t("main.new-version-notification-body").replace("{{link}}", &url);
                app.balloon_url = Some(url);
                balloon(app, &title, &body);
            }
        }
        WM_TIMER => {
            match wp {
                GESTURE_TIMER => {
                    KillTimer(hwnd, GESTURE_TIMER);
                    if let Some(pie) = app.pie.as_mut().filter(|p| !p.hidden) {
                        pie.gesture_timeout();
                        handle_events(app);
                    }
                    frame(app);
                }
                PAD_TIMER => poll_pad(app),
                HOLD_TIMER => {
                    KillTimer(hwnd, HOLD_TIMER);
                    if app.waiting.take().is_some() {
                        if let Some(pie) = app.pie.as_mut() {
                            pie.hide();
                        }
                        handle_events(app);
                        frame(app);
                    }
                }
                _ => frame(app),
            }
        }
        WM_COPYDATA => {
            if lp == 0 {
                return 0;
            }
            let data = &*(lp as *const COPYDATASTRUCT);
            if data.lpData.is_null() || data.cbData < 2 || data.lpData as usize % 2 != 0 {
                return 0;
            }
            let text = std::slice::from_raw_parts(data.lpData as *const u16, data.cbData as usize / 2);
            let text = String::from_utf16_lossy(text).trim_end_matches('\0').to_string();
            dispatch(app, &text);
            return 1;
        }
        TRAY => {
            let event = lp as u32 & 0xFFFF;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP {
                tray_menu(app);
            }
            if event == NIN_BALLOONUSERCLICK {
                if let Some(url) = app.balloon_url.take() {
                    act::open_path(&url);
                }
            }
        }
        WM_DESTROY => PostQuitMessage(0),
        _ => return DefWindowProcW(hwnd, msg, wp, lp),
    }
    0
}
