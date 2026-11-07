use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tiny_skia::Pixmap;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Root,
    Submenu,
    Button,
}

#[derive(Clone)]
pub struct Item {
    pub name: String,
    pub icon: String,
    pub theme: String,
    pub img: Option<Arc<Pixmap>>,
    pub kind: Kind,
    pub angle: Option<f32>,
    pub select: Vec<Value>,
    pub open: Vec<Value>,
    pub activate: Vec<Value>,
    pub hover: Vec<Value>,
    pub key: Option<String>,
    pub activate_key: Option<String>,
    pub children: Vec<Item>,
    pub remote: bool,
}

pub struct Menu {
    pub root: Item,
    pub shortcut: String,
    pub fixed: Option<(f32, f32)>,
    pub anchored: bool,
    pub hover_mode: bool,
    pub conditions: Value,
}

pub struct Settings {
    pub fade_in: f32,
    pub fade_out: f32,
    pub animations: bool,
    pub dead_zone: f32,
    pub parent_distance: f32,
    pub drag_threshold: f32,
    pub marking: bool,
    pub turbo: bool,
    pub warp: bool,
    pub max_radius: f32,
    pub reactive: bool,
    pub underline: bool,
    pub quick_keys: bool,
    pub wedges: bool,
    pub rmb_parent: bool,
    pub zoom: f32,
    pub stroke_len: f32,
    pub stroke_angle: f32,
    pub jitter: f32,
    pub pause: u32,
    pub fixed_stroke: f32,
    pub center_on_release: bool,
    pub alt_click: bool,
    pub same_shortcut: String,
    pub tray: String,
    pub return_pointer: bool,
    pub sound_theme: String,
    pub volume: f32,
    pub gamepad: bool,
    pub back_button: i64,
    pub close_button: i64,
    pub hide_button: bool,
    pub button_position: String,
    pub locale: String,
    pub version_check: bool,
    pub theme: crate::theme::Theme,
}

fn workflow(v: &Value) -> (Vec<Value>, Option<String>) {
    let actions = v["actions"].as_array().cloned().unwrap_or_default();
    let key = v["quickSelectKey"].as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
    (actions, key)
}

fn old(kind: &str, d: &Value) -> Option<Value> {
    let a = match kind {
        "command" => json!({ "type": "execute-command", "command": d["command"] }),
        "uri" => json!({ "type": "open-uri", "uri": d["uri"] }),
        "file" => json!({ "type": "open-file", "path": d["path"] }),
        "hotkey" => json!({ "type": "simulate-hotkey", "hotkey": d["hotkey"] }),
        "macro" => json!({ "type": "execute-macro", "macro": d["macro"] }),
        "text" => json!({ "type": "set-clipboard", "text": d["text"] }),
        "redirect" => json!({ "type": "open-menu", "menu": d["menu"] }),
        _ => return None,
    };
    Some(a)
}

fn item(v: &Value, root: bool) -> Item {
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    let t = s("type");
    let mut children: Vec<Item> = v["children"]
        .as_array()
        .map(|a| a.iter().map(|c| item(c, false)).collect())
        .unwrap_or_default();

    let mut angles: Vec<Option<f32>> = children.iter().map(|c| c.angle).collect();
    crate::math::fix_fixed_angles(&mut angles);
    for (c, a) in children.iter_mut().zip(angles) {
        c.angle = a;
    }

    let kind = if root {
        Kind::Root
    } else if t == "submenu" || (t.is_empty() && !children.is_empty()) {
        Kind::Submenu
    } else {
        Kind::Button
    };

    let (mut select, select_key) = workflow(&v["selectWorkflow"]);
    let (open, open_key) = workflow(&v["openWorkflow"]);
    let (mut activate, activate_key) = workflow(&v["activateWorkflow"]);
    let (hover, _) = workflow(&v["hoverWorkflow"]);

    let data = if v["data"].is_object() { &v["data"] } else { v };
    if let Some(a) = old(&t, data) {
        select = vec![a, json!({ "type": "close-menu" })];
    }
    if v["activateWorkflow"].is_null() {
        match kind {
            Kind::Root => activate = vec![json!({ "type": "close-menu" })],
            Kind::Submenu => activate = vec![json!({ "type": "close-submenu" })],
            Kind::Button => {}
        }
    }

    let icon = s("icon");
    let img = icon
        .split_once("base64,")
        .and_then(|(_, b)| STANDARD.decode(b.trim()).ok())
        .and_then(|b| Pixmap::decode_png(&b).ok())
        .map(Arc::new);

    Item {
        name: s("name"),
        theme: s("iconTheme"),
        icon,
        img,
        kind,
        angle: v["angle"].as_f64().map(|a| a as f32),
        select,
        open,
        activate,
        activate_key,
        hover,
        key: if kind == Kind::Button { select_key } else { open_key },
        children,
        remote: false,
    }
}

fn mark_remote(item: &mut Item) {
    item.remote = true;
    for child in &mut item.children {
        mark_remote(child);
    }
}

pub fn dir() -> PathBuf {
    std::env::current_exe().unwrap().parent().unwrap().to_path_buf()
}

pub fn path() -> PathBuf {
    let near = dir().join("menus.json");
    if near.exists() {
        return near;
    }
    if let Ok(a) = std::env::var("APPDATA") {
        let kando = PathBuf::from(a).join("kando");
        if kando.join("menus.json").exists() {
            if std::fs::copy(kando.join("menus.json"), &near).is_err() {
                return kando.join("menus.json");
            }
            if !near.with_file_name("config.json").exists() {
                let _ = std::fs::copy(kando.join("config.json"), near.with_file_name("config.json"));
            }
        }
    }
    near
}

fn read(p: &PathBuf) -> Value {
    let text = std::fs::read_to_string(p).unwrap_or_default();
    serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or(Value::Null)
}

pub fn custom(root: &Value) -> Menu {
    let mut root = item(root, true);
    mark_remote(&mut root);
    Menu {
        root,
        shortcut: String::new(),
        fixed: None,
        anchored: false,
        hover_mode: false,
        conditions: Value::Null,
    }
}

pub fn load() -> Vec<Menu> {
    let p = path();
    if !p.exists() {
        let _ = std::fs::write(&p, serde_json::to_string_pretty(&example()).unwrap_or_default());
    }
    let mut v = read(&p);
    if v.is_null() {
        v = example();
    }

    let mut menus = vec![];
    for m in v["menus"].as_array().into_iter().flatten() {
        let fp = &m["fixedMenuPosition"];
        let centered = m["centered"].as_bool().unwrap_or(false);
        let fixed = if m["useFixedPosition"].as_bool().unwrap_or(false) {
            Some((fp["x"].as_f64().unwrap_or(0.5) as f32, fp["y"].as_f64().unwrap_or(0.5) as f32))
        } else if centered {
            Some((0.5, 0.5))
        } else {
            None
        };
        menus.push(Menu {
            root: item(&m["root"], true),
            shortcut: m["shortcut"].as_str().unwrap_or("").to_string(),
            fixed,
            anchored: m["anchored"].as_bool().unwrap_or(false),
            hover_mode: m["hoverMode"].as_bool().unwrap_or(false),
            conditions: m["conditions"].clone(),
        });
    }
    menus
}

pub fn config_path() -> PathBuf {
    path().with_file_name("config.json")
}

pub fn config() -> Value {
    let v = read(&config_path());
    if v.is_object() {
        v
    } else {
        Value::Object(Default::default())
    }
}

pub fn settings() -> Settings {
    let c = config();
    let f = |k: &str, d: f64| c[k].as_f64().unwrap_or(d) as f32;
    let b = |k: &str, d: bool| c[k].as_bool().unwrap_or(d);
    let st = |k: &str, d: &str| c[k].as_str().unwrap_or(d).to_string();

    Settings {
        fade_in: f("fadeInDuration", 75.0),
        fade_out: f("fadeOutDuration", 100.0),
        animations: b("enableMenuAnimations", true),
        dead_zone: f("centerDeadZone", 50.0),
        parent_distance: f("minParentDistance", 150.0),
        drag_threshold: f("dragThreshold", 15.0),
        marking: b("enableMarkingMode", true),
        turbo: b("enableTurboMode", true) && !b("keepInputFocus", false),
        warp: b("warpMouse", true),
        max_radius: f("maxSelectionRadius", 0.0),
        reactive: b("enablePointerReactiveEffects", true),
        underline: b("underlineQuickSelectKey", true),
        quick_keys: b("drawQuickSelectKey", false),
        wedges: b("enableSelectionWedges", false),
        rmb_parent: b("rmbSelectsParent", false),
        zoom: f("zoomFactor", 1.0),
        stroke_len: f("gestureMinStrokeLength", 150.0),
        stroke_angle: f("gestureMinStrokeAngle", 20.0),
        jitter: f("gestureJitterThreshold", 10.0),
        pause: f("gesturePauseTimeout", 100.0) as u32,
        fixed_stroke: f("fixedStrokeLength", 0.0),
        center_on_release: b("triggerCenterClickOnKeyRelease", false),
        alt_click: b("altClick", true),
        same_shortcut: st("sameShortcutBehavior", "nothing"),
        tray: st("trayIconFlavor", "color"),
        return_pointer: b("returnPointerToMenuOpeningPosition", false),
        sound_theme: st("soundTheme", "none"),
        volume: f("soundVolume", 0.5),
        gamepad: b("enableGamepad", true),
        back_button: c["gamepadBackButton"].as_i64().unwrap_or(1),
        close_button: c["gamepadCloseButton"].as_i64().unwrap_or(2),
        hide_button: b("hideSettingsButton", false),
        button_position: st("settingsButtonPosition", "bottom-right"),
        locale: st("locale", "auto"),
        version_check: b("enableVersionCheck", true),
        theme: crate::theme::load(&c),
    }
}

const EXAMPLE: &str = include_str!("../assets/example-menu.json");

fn translate(item: &mut Value, tr: &crate::tr::Tr) {
    if let Some(name) = item["name"].as_str() {
        if name.starts_with("example-menu.") {
            item["name"] = json!(tr.t(name));
        }
    }
    if let Some(children) = item["children"].as_array_mut() {
        for child in children {
            translate(child, tr);
        }
    }
}

pub fn example() -> Value {
    let mut v: Value = serde_json::from_str(EXAMPLE).unwrap_or(Value::Null);
    let locale = config()["locale"].as_str().unwrap_or("auto").to_string();
    let tr = crate::tr::Tr::new(&locale);
    if let Some(menus) = v["menus"].as_array_mut() {
        for m in menus {
            translate(&mut m["root"], &tr);
        }
    }
    v
}
