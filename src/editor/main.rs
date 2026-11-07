#![windows_subsystem = "windows"]

#[allow(dead_code)]
#[path = "../math.rs"]
mod math;
#[allow(dead_code)]
#[allow(dead_code)]
#[path = "../apps.rs"]
mod apps;
#[allow(dead_code)]
#[path = "../theme.rs"]
mod theme;
#[allow(dead_code)]
#[path = "../sound.rs"]
mod sound;

mod dialogs;
mod icons;
mod keys;
mod preview;
mod props;
mod style;

use style::colors;
#[path = "../tr.rs"]
mod tr;

use eframe::egui::{self, Align2, Color32, CornerRadius, FontFamily, FontId, Rect, RichText, Sense, Stroke, Vec2};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use icons::Icons;
use tr::Tr;

pub const STAR: &str = include_str!("../../assets/brand/star.svg");
pub const REPOSITORY: &str = "https://github.com/Goldingeng/Orbit-Actions";

#[derive(Clone, PartialEq)]
pub enum Dialog {
    None,
    Settings,
    Themes,
    About,
    Icon(Vec<usize>),
    CollectionIcon(usize),
    Action(String),
    Intro,
    Apps(Pick),
    Window(Pick),
}

#[derive(Clone, PartialEq)]
pub enum Pick {
    Command(String, usize),
    FocusApp(String, usize),
    FocusWindow(String, usize),
    AppCondition,
    WindowCondition,
}

#[derive(Clone, PartialEq)]
pub enum Recording {
    None,
    Shortcut,
    QuickKey(String),
    Hotkey(String, usize),
    Macro(String, usize),
}

pub struct Editor {
    pub doc: Value,
    pub config: Value,
    committed: Value,
    saved_config: Value,
    menus_path: PathBuf,
    config_path: PathBuf,
    undo: Vec<Value>,
    redo: Vec<Value>,
    pending: Option<Instant>,

    pub menu: usize,
    pub stack: Vec<usize>,
    pub selected: Option<usize>,
    pub collection: Option<usize>,
    pub search: Option<String>,

    pub dialog: Dialog,
    pub icon_theme: String,
    pub icon_search: String,
    pub action_search: String,
    pub workflow_tab: String,
    pub recording: Recording,
    pub macro_mods: egui::Modifiers,

    pub drag: Option<preview::Drag>,
    pub motion: preview::Motion,
    pub icons: Icons,
    pub tr: Tr,
    pub settings_tab: usize,
    pub slide: usize,
    pub dark_variant: bool,
    pub app_search: String,
    pub app_choice: Option<String>,
    pub countdown: Option<Instant>,
    pub picked: Option<(String, String)>,
}

pub fn data_dir() -> PathBuf {
    std::env::current_exe().unwrap().parent().unwrap().to_path_buf()
}

fn read_json(path: &PathBuf) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or(Value::Null)
}

fn risky_actions(item: &Value) -> Vec<String> {
    let name = item["name"].as_str().unwrap_or("");
    let data = if item["data"].is_object() { &item["data"] } else { item };
    let legacy = json!({ "type": item["type"].as_str().map(|t| match t {
        "command" => "execute-command",
        "file" => "open-file",
        "hotkey" => "simulate-hotkey",
        "macro" => "execute-macro",
        "uri" => "open-uri",
        other => other,
    }), "command": data["command"], "path": data["path"], "hotkey": data["hotkey"], "uri": data["uri"] });
    let workflows = ["selectWorkflow", "hoverWorkflow", "openWorkflow", "activateWorkflow"];
    let actions = workflows.iter().flat_map(|w| item[*w]["actions"].as_array().into_iter().flatten()).chain(std::iter::once(&legacy));
    let mut found: Vec<String> = actions
        .filter_map(|action| {
            let detail = match action["type"].as_str()? {
                "execute-command" => action["command"].as_str().unwrap_or("").to_string(),
                "open-file" => action["path"].as_str().unwrap_or("").to_string(),
                "send-websocket-message" => action["url"].as_str().unwrap_or("").to_string(),
                "simulate-hotkey" => action["hotkey"].as_str().unwrap_or("").to_string(),
                "open-uri" => {
                    let uri = action["uri"].as_str().unwrap_or("");
                    let scheme = uri.split_once(':').map(|(s, _)| s.to_ascii_lowercase()).unwrap_or_default();
                    if matches!(scheme.as_str(), "http" | "https" | "mailto") {
                        return None;
                    }
                    uri.to_string()
                }
                "execute-macro" => "macro".to_string(),
                _ => return None,
            };
            Some(format!("{name}: {detail}"))
        })
        .collect();
    for child in item["children"].as_array().into_iter().flatten() {
        found.extend(risky_actions(child));
    }
    found
}

fn write_json(path: &PathBuf, value: &Value) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    let mut temp = path.clone().into_os_string();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

pub fn notify_daemon() {
    send_daemon("--reload");
}

pub fn send_daemon(message: &str) {
    use windows_sys::Win32::Foundation::LPARAM;
    use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SendMessageW, WM_COPYDATA};
    let class: Vec<u16> = "orbit-actions".encode_utf16().chain(Some(0)).collect();
    let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let hwnd = FindWindowW(class.as_ptr(), std::ptr::null());
        if !hwnd.is_null() {
            let data = COPYDATASTRUCT { dwData: 1, cbData: (text.len() * 2) as u32, lpData: text.as_ptr() as _ };
            SendMessageW(hwnd, WM_COPYDATA, 0, &data as *const _ as LPARAM);
        }
    }
}

pub fn item_type(item: &Value) -> &str {
    item["type"].as_str().unwrap_or("button")
}

pub fn round_button(ui: &mut egui::Ui, icon: char, size: f32, tooltip: &str) -> egui::Response {
    let tone = colors(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let fill = if response.hovered() { tone.line } else { Color32::TRANSPARENT };
    ui.painter().circle_filled(rect.center(), size / 2.0, fill);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, icons::material_font(size * 0.6), tone.text);
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

pub fn pill(ui: &mut egui::Ui, text: &str, icon: Option<char>) -> egui::Response {
    let tone = colors(ui.ctx());
    let label = match icon {
        Some(i) => format!("{i}  {text}"),
        None => text.to_string(),
    };
    let galley_width = ui.fonts(|f| f.layout_no_wrap(label.clone(), FontId::proportional(17.0), tone.text).size().x);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(galley_width + 44.0, 44.0), Sense::click());
    let fill = if response.hovered() { tone.hover } else { tone.button };
    ui.painter().rect_filled(rect, CornerRadius::same(22), fill);
    let mut x = rect.left() + 22.0;
    if let Some(i) = icon {
        ui.painter().text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, i, icons::material_font(20.0), tone.text);
        x += 26.0;
    }
    ui.painter().text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, text, FontId::proportional(17.0), tone.text);
    response
}

fn backdrop(hwnd: windows_sys::Win32::Foundation::HWND, light: bool) {
    use windows_sys::Win32::Graphics::Dwm::*;
    use windows_sys::Win32::UI::Controls::MARGINS;
    let dark = i32::from(!light);
    let kind = DWMSBT_MAINWINDOW;
    let margins = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
    unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as _, &dark as *const i32 as _, 4);
        DwmExtendFrameIntoClientArea(hwnd, &margins);
        DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE as _, &kind as *const i32 as _, 4);
    }
}

fn has_tags(menu: &Value, collection: &Value) -> bool {
    let tags: Vec<&str> = menu["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str()).collect()).unwrap_or_default();
    collection["tags"].as_array().into_iter().flatten().filter_map(|t| t.as_str()).all(|w| tags.contains(&w))
}

impl Editor {
    fn new(cc: &eframe::CreationContext) -> Editor {
        icons::install_fonts(&cc.egui_ctx);
        let dir = data_dir();
        let menus_path = dir.join("menus.json");
        let config_path = dir.join("config.json");
        let mut doc = read_json(&menus_path);
        if !doc["menus"].is_array() {
            doc = json!({ "version": "3.0.0", "menus": [], "collections": [] });
        }
        let config = read_json(&config_path);
        let config = if config.is_object() { config } else { json!({}) };
        let locale = config["locale"].as_str().unwrap_or("auto").to_string();
        let scheme = config["settingsWindowColorScheme"].as_str().unwrap_or("system");
        let light = scheme == "light" || (scheme == "system" && !theme::system_dark());
        style::apply(&cc.egui_ctx, !light);
        if let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(cc) {
            if let raw_window_handle::RawWindowHandle::Win32(win) = handle.as_raw() {
                backdrop(win.hwnd.get() as _, light);
            }
        }
        let intro = config["showIntroductionDialog"].as_bool().unwrap_or(true);

        let mut editor = Editor {
            committed: doc.clone(),
            saved_config: config.clone(),
            doc,
            config,
            menus_path,
            config_path,
            undo: vec![],
            redo: vec![],
            pending: None,
            menu: 0,
            stack: vec![],
            selected: None,
            collection: None,
            search: None,
            dialog: Dialog::None,
            icon_theme: "material-symbols-rounded".into(),
            icon_search: String::new(),
            action_search: String::new(),
            workflow_tab: String::new(),
            recording: Recording::None,
            macro_mods: egui::Modifiers::NONE,
            drag: None,
            motion: preview::Motion::default(),
            icons: Icons::new(&cc.egui_ctx),
            tr: Tr::new(&locale),
            settings_tab: 0,
            slide: 0,
            dark_variant: false,
            app_search: String::new(),
            app_choice: None,
            countdown: None,
            picked: None,
        };
        if intro {
            editor.dialog = Dialog::Intro;
        }
        let args: Vec<String> = std::env::args().collect();
        if args.iter().any(|a| a == "--themes") {
            editor.dialog = Dialog::Themes;
        } else if args.iter().any(|a| a == "--general") {
            editor.dialog = Dialog::Settings;
        }
        editor
    }

    pub fn t(&self, key: &str) -> String {
        self.tr.t(key)
    }

    pub fn menus(&self) -> &[Value] {
        self.doc["menus"].as_array().map(|a| a.as_slice()).unwrap_or(&[])
    }

    pub fn node(&self, path: &[usize]) -> Option<&Value> {
        let mut item = self.doc["menus"].get(self.menu)?.get("root")?;
        for &i in path {
            item = item.get("children")?.get(i)?;
        }
        Some(item)
    }

    pub fn node_mut(&mut self, path: &[usize]) -> Option<&mut Value> {
        let mut item = self.doc["menus"].get_mut(self.menu)?.get_mut("root")?;
        for &i in path {
            item = item.get_mut("children")?.get_mut(i)?;
        }
        Some(item)
    }

    pub fn selected_path(&self) -> Vec<usize> {
        let mut path = self.stack.clone();
        if let Some(s) = self.selected {
            path.push(s);
        }
        path
    }

    pub fn open_menu(&mut self, index: usize) {
        self.menu = index;
        self.stack.clear();
        self.selected = None;
    }

    fn do_undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.doc, prev));
            self.committed = self.doc.clone();
            self.fix_selection();
            self.save_menus();
        }
    }

    fn do_redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.doc, next));
            self.committed = self.doc.clone();
            self.fix_selection();
            self.save_menus();
        }
    }

    pub fn fix_selection(&mut self) {
        let count = self.menus().len();
        if self.menu >= count {
            self.menu = count.saturating_sub(1);
            self.stack.clear();
            self.selected = None;
        }
        while !self.stack.is_empty() && self.node(&self.stack.clone()).is_none() {
            self.stack.pop();
            self.selected = None;
        }
        if let Some(s) = self.selected {
            let len = self.node(&self.stack.clone()).and_then(|n| n["children"].as_array()).map_or(0, |c| c.len());
            if s >= len {
                self.selected = None;
            }
        }
    }

    fn save_failed(&self, path: &PathBuf, error: std::io::Error) {
        let message = self.t("settings.save-error-message").replace("{{file}}", &path.display().to_string()).replace("{{error}}", &error.to_string());
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title(self.t("settings.save-error-title"))
            .set_description(message)
            .show();
    }

    fn save_menus(&mut self) {
        if let Err(error) = write_json(&self.menus_path, &self.doc) {
            self.save_failed(&self.menus_path, error);
        }
        notify_daemon();
    }

    fn checkpoint(&mut self, ctx: &egui::Context) {
        if self.config != self.saved_config {
            if let Err(error) = write_json(&self.config_path, &self.config) {
                self.save_failed(&self.config_path, error);
            }
            self.saved_config = self.config.clone();
            notify_daemon();
        }
        if self.doc == self.committed {
            self.pending = None;
            return;
        }
        let typing = ctx.memory(|m| m.focused().is_some());
        let since = *self.pending.get_or_insert_with(Instant::now);
        if !typing || since.elapsed() > Duration::from_millis(700) {
            let before = std::mem::replace(&mut self.committed, self.doc.clone());
            self.undo.push(before);
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.pending = None;
            self.save_menus();
        } else {
            ctx.request_repaint_after(Duration::from_millis(750));
        }
    }

    pub fn new_menu(&mut self) {
        let menu = json!({
            "root": {
                "type": "root",
                "name": self.t("settings.new-menu-name"),
                "icon": "apps",
                "iconTheme": "material-symbols-rounded",
                "children": [],
                "activateWorkflow": { "quickSelectKey": "Backspace", "actions": [{ "type": "close-menu" }] }
            },
            "shortcut": "",
            "shortcutID": "",
            "centered": false,
            "anchored": false,
            "hoverMode": false,
            "tags": self.collection.and_then(|c| self.doc["collections"].get(c)).map(|c| c["tags"].clone()).unwrap_or(json!([]))
        });
        if let Some(list) = self.doc["menus"].as_array_mut() {
            list.push(menu);
            let last = list.len() - 1;
            self.open_menu(last);
        }
    }

    fn menu_matches(&self, menu: &Value) -> bool {
        if let Some(c) = self.collection {
            if !has_tags(menu, &self.doc["collections"][c]) {
                return false;
            }
        }
        match &self.search {
            Some(s) if !s.is_empty() => {
                let s = s.to_lowercase();
                menu["root"]["name"].as_str().unwrap_or("").to_lowercase().contains(&s)
                    || menu["shortcut"].as_str().unwrap_or("").to_lowercase().contains(&s)
            }
            _ => true,
        }
    }

    fn rail(&mut self, ui: &mut egui::Ui) {
    let tone = colors(ui.ctx());
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(58.0), Sense::click());
            ui.painter().circle_filled(rect.center(), 29.0, Color32::from_rgb(0x14, 0x14, 0x14));
            ui.painter().circle_stroke(rect.center(), 29.0, Stroke::new(2.0, tone.text));
            let logo = rect.shrink(11.0);
            let px = (logo.width() * ui.ctx().pixels_per_point()).round() as u32;
            if let Some(tex) = self.icons.svg_texture(ui.ctx(), "star", STAR, px) {
                ui.painter().image(tex.id(), logo, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            }
            if response.on_hover_text(self.t("settings.about-dialog.title")).clicked() {
                self.dialog = Dialog::About;
            }
            ui.add_space(14.0);

            let all = self.collection.is_none();
            let grid = self.icons.glyph("grid_view");
            let r = round_button(ui, grid, 50.0, &self.t("settings.all-menus"));
            if all {
                ui.painter().circle_stroke(r.rect.center(), 25.0, Stroke::new(1.5, tone.text));
            }
            if r.clicked() {
                self.collection = None;
            }

            let collections: Vec<Value> = self.doc["collections"].as_array().cloned().unwrap_or_default();
            for (i, c) in collections.iter().enumerate() {
                ui.add_space(6.0);
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(50.0), Sense::click());
                if self.collection == Some(i) {
                    ui.painter().circle_stroke(rect.center(), 25.0, Stroke::new(1.5, tone.text));
                }
                let (theme, icon) = self.collection_icon(c);
                self.icons.draw(ui.ctx(), ui.painter(), rect.shrink(8.0), &theme, &icon, tone.text);
                let response = response.on_hover_text(self.collection_name(c));
                if response.clicked() {
                    self.collection = Some(i);
                }
                response.context_menu(|ui| {
                    if ui.button(self.tr.t("settings.delete-menu")).clicked() {
                        if let Some(list) = self.doc["collections"].as_array_mut() {
                            list.remove(i);
                        }
                        self.collection = None;
                        ui.close_menu();
                    }
                });
            }
            ui.add_space(6.0);
            let plus = self.icons.glyph("add");
            if round_button(ui, plus, 50.0, &self.t("settings.add-collection-tooltip")).clicked() {
                if !self.doc["collections"].is_array() {
                    self.doc["collections"] = json!([]);
                }
                let name = format!("{} {}", self.t("settings.new-collection-name"), self.doc["collections"].as_array().map_or(0, |a| a.len()) + 1);
                let root = &self.doc["menus"][self.menu]["root"];
                let icon = root["icon"].as_str().unwrap_or("sell").to_string();
                let theme = root["iconTheme"].as_str().unwrap_or("material-symbols-rounded").to_string();
                self.doc["collections"].as_array_mut().unwrap().push(json!({
                    "name": name, "icon": icon, "iconTheme": theme, "tags": [name.to_lowercase()]
                }));
                self.collection = Some(self.doc["collections"].as_array().unwrap().len() - 1);
            }
        });
    }

    fn collection_icon(&self, c: &Value) -> (String, String) {
        let first = self.menus().iter().find(|m| has_tags(m, c)).map(|m| &m["root"]);
        let source = if c["icon"] == "sell" { first.unwrap_or(c) } else { c };
        let theme = source["iconTheme"].as_str().unwrap_or("material-symbols-rounded").to_string();
        let icon = source["icon"].as_str().unwrap_or("sell").to_string();
        (theme, icon)
    }

    fn collection_name(&self, c: &Value) -> String {
        let name = c["name"].as_str().unwrap_or("");
        let tags = c["tags"].as_array();
        let favorite = matches!(name, "Favorites" | "Favorit" | "Favourite" | "Favourites")
            && c["icon"].as_str() == Some("favorite")
            && tags.is_some_and(|tags| tags.len() == 1 && tags[0].as_str() == Some("favs"));
        if favorite { self.t("settings.favorites") } else { name.to_string() }
    }

    fn menu_list(&mut self, ui: &mut egui::Ui) {
    let tone = colors(ui.ctx());
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let title = match self.collection {
                Some(c) => self.collection_name(&self.doc["collections"][c]),
                None => self.t("settings.all-menus"),
            };
            ui.label(RichText::new(title).font(FontId::new(22.0, FontFamily::Name("bold".into()))).color(tone.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let search = self.icons.glyph("search");
                if round_button(ui, search, 34.0, "").clicked() {
                    self.search = if self.search.is_some() { None } else { Some(String::new()) };
                    if self.search.is_some() {
                        ui.memory_mut(|m| m.request_focus(egui::Id::new("menu-search")));
                    }
                }
            });
        });

        if let Some(c) = self.collection {
            let mut name = self.collection_name(&self.doc["collections"][c]);
            ui.horizontal(|ui| {
                let (theme, icon) = self.collection_icon(&self.doc["collections"][c]);
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
                if response.hovered() {
                    ui.painter().circle_filled(rect.center(), 15.0, tone.line);
                }
                self.icons.draw(ui.ctx(), ui.painter(), rect.shrink(4.0), &theme, &icon, tone.text);
                if response.clicked() {
                    self.icon_theme = theme;
                    self.icon_search.clear();
                    self.dialog = Dialog::CollectionIcon(c);
                }
                if ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY)).changed() {
                    self.doc["collections"][c]["name"] = json!(name);
                }
            });
            let mut tags = self.doc["collections"][c]["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
            if ui.add(egui::TextEdit::singleline(&mut tags).hint_text(self.t("settings.add-tags-placeholder")).desired_width(f32::INFINITY)).changed() {
                let list: Vec<Value> = tags.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()).map(|t| json!(t)).collect();
                self.doc["collections"][c]["tags"] = Value::Array(list);
            }
        }

        let placeholder = self.t("settings.search-menus-placeholder");
        if let Some(search) = self.search.as_mut() {
            ui.add(egui::TextEdit::singleline(search).id(egui::Id::new("menu-search")).hint_text(placeholder).desired_width(f32::INFINITY));
        }
        ui.add_space(8.0);

        let visible: Vec<usize> = (0..self.menus().len()).filter(|&i| self.menu_matches(&self.menus()[i])).collect();
        egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(ui.available_height() - 70.0).show(ui, |ui| {
            if visible.is_empty() {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    let (title, note) = if self.menus().is_empty() {
                        ("settings.no-menus", "settings.no-menus-note")
                    } else if self.collection.is_some() && self.search.as_deref().unwrap_or("").is_empty() {
                        ("settings.empty-collection", "settings.empty-collection-note")
                    } else {
                        ("settings.no-matching-menus", "settings.no-matching-menus-note")
                    };
                    ui.label(RichText::new(self.t(title)).strong());
                    ui.label(RichText::new(self.t(note)).color(tone.dim));
                });
            }
            for i in visible {
                if self.menu_card(ui, i) {
                    break;
                }
                ui.add_space(6.0);
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(30.0);
            let add = self.icons.glyph("add");
            if pill(ui, &self.t("settings.create-menu-button"), Some(add)).clicked() {
                self.new_menu();
            }
            let import = self.icons.glyph("download");
            if pill(ui, "", Some(import)).on_hover_text(self.t("settings.import-menu")).clicked() {
                self.import_menu();
            }
        });
    }

    fn menu_card(&mut self, ui: &mut egui::Ui, i: usize) -> bool {
    let tone = colors(ui.ctx());
        let menu = self.menus()[i].clone();
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 74.0), Sense::click());
        let selected = i == self.menu;
        let fill = if selected { tone.card } else if response.hovered() { tone.card } else { tone.panel };
        ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
        if selected {
            ui.painter().rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, tone.dim), egui::StrokeKind::Inside);
        }

        let icon_rect = Rect::from_center_size(egui::pos2(rect.left() + 34.0, rect.center().y), Vec2::splat(34.0));
        let theme = menu["root"]["iconTheme"].as_str().unwrap_or("").to_string();
        let icon = menu["root"]["icon"].as_str().unwrap_or("").to_string();
        self.icons.draw(ui.ctx(), ui.painter(), icon_rect, &theme, &icon, tone.text);

        let name = menu["root"]["name"].as_str().unwrap_or("").to_string();
        let shortcut = menu["shortcut"].as_str().unwrap_or("").to_string();
        let shortcut = if shortcut.is_empty() { self.t("settings.not-bound") } else { shortcut };
        ui.painter().text(egui::pos2(rect.left() + 64.0, rect.center().y - 10.0), Align2::LEFT_CENTER, name, FontId::proportional(19.0), tone.text);
        ui.painter().text(egui::pos2(rect.left() + 64.0, rect.center().y + 13.0), Align2::LEFT_CENTER, shortcut, FontId::proportional(14.0), tone.dim);

        let mut x = rect.right() - 28.0;
        let mut tools = vec![];
        for (glyph, tip) in [("delete", "settings.delete-menu"), ("content_copy", "settings.duplicate-menu"), ("upload", "settings.export-menu")] {
            let r = Rect::from_center_size(egui::pos2(x, rect.center().y), Vec2::splat(30.0));
            let resp = ui.interact(r, ui.id().with(("menu-tool", i, glyph)), Sense::click());
            if selected || response.hovered() || resp.hovered() {
                if resp.hovered() {
                    ui.painter().circle_filled(r.center(), 15.0, tone.line);
                }
                ui.painter().text(r.center(), Align2::CENTER_CENTER, self.icons.glyph(glyph), icons::material_font(19.0), tone.text);
            }
            tools.push((glyph, resp.on_hover_text(self.t(tip))));
            x -= 38.0;
        }

        for (glyph, resp) in tools {
            if resp.clicked() {
                match glyph {
                    "delete" => {
                        if let Some(list) = self.doc["menus"].as_array_mut() {
                            list.remove(i);
                        }
                        self.fix_selection();
                    }
                    "content_copy" => {
                        let mut copy = menu.clone();
                        copy["shortcut"] = json!("");
                        copy["shortcutID"] = json!("");
                        if let Some(list) = self.doc["menus"].as_array_mut() {
                            list.insert(i + 1, copy);
                        }
                        self.open_menu(i + 1);
                    }
                    _ => self.export_menu(i),
                }
                return true;
            }
        }
        if response.clicked() {
            self.open_menu(i);
        }
        false
    }

    fn import_menu(&mut self) {
        let Some(file) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file() else { return };
        let value = read_json(&file);
        let menu = if value["root"].is_object() { value } else if value["menus"].is_array() { value["menus"][0].clone() } else { Value::Null };
        if menu["root"].is_object() {
            let risky = risky_actions(&menu["root"]);
            if !risky.is_empty() {
                let mut list = risky.iter().take(12).cloned().collect::<Vec<_>>().join("\n");
                if risky.len() > 12 {
                    list.push_str(&format!("\n+{}", risky.len() - 12));
                }
                let answer = rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Warning)
                    .set_title(self.t("settings.import-menu-warning-title"))
                    .set_description(self.t("settings.import-menu-warning-message").replace("{{actions}}", &list))
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show();
                if answer != rfd::MessageDialogResult::Yes {
                    return;
                }
            }
            let mut menu = menu;
            menu["shortcut"] = json!("");
            if let Some(list) = self.doc["menus"].as_array_mut() {
                list.push(menu);
                let last = list.len() - 1;
                self.open_menu(last);
            }
        } else {
            rfd::MessageDialog::new()
                .set_title(self.t("settings.import-menu-error-title"))
                .set_description(self.t("settings.import-menu-error-message"))
                .show();
        }
    }

    fn export_menu(&mut self, i: usize) {
        let menu = self.menus()[i].clone();
        let name = menu["root"]["name"].as_str().unwrap_or("menu").to_string();
        if let Some(file) = rfd::FileDialog::new().add_filter("JSON", &["json"]).set_file_name(format!("{name}.json")).save_file() {
            if let Err(error) = write_json(&file, &menu) {
                self.save_failed(&file, error);
            }
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let width = 6.0 * 44.0;
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - width).max(0.0) / 2.0);
            let buttons = [
                ("school", "settings.introduction-dialog.title"),
                ("info", "settings.about-dialog.title"),
                ("palette", "settings.menu-themes-dialog.title"),
                ("settings", "settings.general-settings-dialog.title"),
                ("undo", "settings.undo"),
                ("redo", "settings.redo"),
            ];
            for (glyph, tip) in buttons {
                let enabled = match glyph {
                    "undo" => !self.undo.is_empty(),
                    "redo" => !self.redo.is_empty(),
                    _ => true,
                };
                let g = self.icons.glyph(glyph);
                let r = ui.add_enabled_ui(enabled, |ui| round_button(ui, g, 38.0, &self.tr.t(tip))).inner;
                if r.clicked() {
                    match glyph {
                        "school" => {
                            self.slide = 0;
                            self.dialog = Dialog::Intro;
                        }
                        "info" => self.dialog = Dialog::About,
                        "palette" => self.dialog = Dialog::Themes,
                        "settings" => self.dialog = Dialog::Settings,
                        "undo" => self.do_undo(),
                        _ => self.do_redo(),
                    }
                }
                ui.add_space(6.0);
            }
        });
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.recording != Recording::None || ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let (undo, redo) = ctx.input(|i| {
            let z = i.modifiers.command && i.key_pressed(egui::Key::Z);
            (z && !i.modifiers.shift, (z && i.modifiers.shift) || (i.modifiers.command && i.key_pressed(egui::Key::Y)))
        });
        if undo {
            self.do_undo();
        }
        if redo {
            self.do_redo();
        }
    }
}

pub fn open_url(url: &str) {
    let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
}

impl eframe::App for Editor {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let scheme = self.config["settingsWindowColorScheme"].as_str().unwrap_or("system");
        let dark = scheme == "dark" || (scheme != "light" && theme::system_dark());
        if ctx.style().visuals.dark_mode != dark {
            style::apply(ctx, dark);
            if let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(_frame) {
                if let raw_window_handle::RawWindowHandle::Win32(win) = handle.as_raw() {
                    backdrop(win.hwnd.get() as _, !dark);
                }
            }
        }
        let tone = colors(ctx);
        self.shortcuts(ctx);
        keys::record(self, ctx);
        self.icons.poll(ctx);
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            if let Some(path) = file.path {
                self.drop_file(&path);
            }
        }

        egui::SidePanel::left("rail")
            .exact_width(84.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(tone.bg))
            .show(ctx, |ui| self.rail(ui));

        egui::SidePanel::left("menus")
            .exact_width(360.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(tone.panel).inner_margin(egui::Margin::symmetric(20, 8)))
            .show(ctx, |ui| self.menu_list(ui));

        egui::SidePanel::right("properties")
            .exact_width(440.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(tone.panel).inner_margin(egui::Margin::symmetric(22, 10)))
            .show(ctx, |ui| self.properties(ui));

        egui::CentralPanel::default().frame(egui::Frame::new().fill(tone.stage)).show(ctx, |ui| {
            ui.add_space(10.0);
            self.header(ui);
            self.preview(ui);
        });

        dialogs::show(self, ctx);
        self.checkpoint(ctx);
    }
}

fn main() -> eframe::Result {
    let icon = icons::svg_image(include_str!("../../assets/brand/app.svg"), 64).map(|img| egui::IconData {
        rgba: img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect(),
        width: 64,
        height: 64,
    });
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([1480.0, 900.0]).with_min_inner_size([1100.0, 680.0]).with_title("Orbit Actions").with_transparent(true);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("Orbit Actions", options, Box::new(|cc| Ok(Box::new(Editor::new(cc)))))
}
