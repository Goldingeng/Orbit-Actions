use crate::style::colors;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontFamily, FontId, RichText, Sense, Stroke, Vec2};
use serde_json::{json, Value};

use crate::{icons, item_type, round_button, Dialog, Editor, Pick, Recording};

pub struct Meta {
    pub icon: &'static str,
    pub delayed: bool,
    pub inhibit: bool,
    kind: &'static str,
}

impl Meta {
    pub fn create(&self) -> Value {
        match self.kind {
            "delay" => json!({ "type": "delay", "duration": 1 }),
            "execute-command" => json!({ "type": "execute-command", "command": "", "detached": true, "isolated": false }),
            "execute-macro" => json!({ "type": "execute-macro", "macro": [] }),
            "focus-window" => json!({ "type": "focus-window", "windowName": "", "appName": "" }),
            "open-file" => json!({ "type": "open-file", "path": "" }),
            "open-menu" => json!({ "type": "open-menu", "menu": "" }),
            "open-uri" => json!({ "type": "open-uri", "uri": "" }),
            "send-websocket-message" => json!({ "type": "send-websocket-message", "url": "", "message": "" }),
            "set-clipboard" => json!({ "type": "set-clipboard", "text": "" }),
            "simulate-hotkey" => json!({ "type": "simulate-hotkey", "hotkey": "" }),
            other => json!({ "type": other }),
        }
    }
}

pub const ACTIONS: &[&str] = &[
    "close-menu",
    "close-submenu",
    "delay",
    "execute-command",
    "execute-macro",
    "focus-window",
    "inhibit-shortcuts",
    "open-file",
    "open-menu",
    "open-settings",
    "open-uri",
    "send-websocket-message",
    "set-clipboard",
    "simulate-hotkey",
];

pub fn action_meta(kind: &str) -> Meta {
    let (kind, icon, delayed, inhibit) = match kind {
        "close-menu" => ("close-menu", "close-menu-item.svg", false, false),
        "close-submenu" => ("close-submenu", "close-submenu-item.svg", false, false),
        "delay" => ("delay", "delay-item.svg", false, false),
        "execute-command" => ("execute-command", "command-item.svg", false, false),
        "execute-macro" => ("execute-macro", "macro-item.svg", true, true),
        "focus-window" => ("focus-window", "focus-window-item.svg", false, false),
        "inhibit-shortcuts" => ("inhibit-shortcuts", "inhibit-shortcuts-item.svg", false, false),
        "open-file" => ("open-file", "file-item.svg", false, false),
        "open-menu" => ("open-menu", "redirect-item.svg", false, false),
        "open-settings" => ("open-settings", "settings-item.svg", false, false),
        "open-uri" => ("open-uri", "uri-item.svg", false, false),
        "set-clipboard" => ("set-clipboard", "clipboard-item.svg", false, false),
        "simulate-hotkey" => ("simulate-hotkey", "hotkey-item.svg", true, true),
        _ => ("send-websocket-message", "uri-item.svg", false, false),
    };
    Meta { icon, delayed, inhibit, kind }
}

fn workflows(kind: &str) -> &'static [(&'static str, &'static str, &'static str)] {
    match kind {
        "root" => &[("activateWorkflow", "activate", "root-activate-workflow")],
        "submenu" => &[
            ("openWorkflow", "open", "submenu-open-workflow"),
            ("hoverWorkflow", "hover", "submenu-hover-workflow"),
            ("activateWorkflow", "activate", "submenu-activate-workflow"),
        ],
        _ => &[("selectWorkflow", "select", "button-select-workflow"), ("hoverWorkflow", "hover", "button-hover-workflow")],
    }
}

fn row(ui: &mut egui::Ui, label: &str, info: &str, add: impl FnOnce(&mut egui::Ui)) {
    let width = ui.available_width();
    let label_width = (width * 0.40).floor();
    let field_width = width - label_width - ui.spacing().item_spacing.x;
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(Vec2::new(label_width, 32.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_width(label_width);
            ui.add(egui::Label::new(RichText::new(label).size(17.0)).wrap()).on_hover_text(info);
        });
        ui.allocate_ui_with_layout(Vec2::new(field_width, 32.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.set_width(field_width);
            add(ui);
        });
    });
}

fn section(ui: &mut egui::Ui, title: String, arrows: [char; 2], add: impl FnOnce(&mut egui::Ui)) {
    let tone = colors(ui.ctx());
    let id = ui.make_persistent_id(&title);
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, ui.is_enabled(), &title));
    if response.clicked() {
        state.toggle(ui);
    }
    if response.hovered() {
        ui.painter().rect_filled(rect, 6, tone.card);
    }
    ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, title, FontId::proportional(18.0), tone.text);
    ui.painter().text(rect.right_center() - Vec2::new(12.0, 0.0), Align2::CENTER_CENTER, arrows[usize::from(state.is_open())], icons::material_font(20.0), tone.dim);
    state.show_body_unindented(ui, add);
}

fn edit_text(ui: &mut egui::Ui, value: &mut Value, key: &str, hint: &str, multiline: bool) {
    let mut text = value[key].as_str().unwrap_or("").to_string();
    let widget = if multiline {
        egui::TextEdit::multiline(&mut text).desired_rows(3)
    } else {
        egui::TextEdit::singleline(&mut text)
    };
    if ui.add(widget.hint_text(hint).desired_width(f32::INFINITY)).changed() {
        value[key] = json!(text);
    }
}

fn edit_bool(ui: &mut egui::Ui, value: &mut Value, key: &str, label: &str, info: &str, default: bool) {
    let mut b = value[key].as_bool().unwrap_or(default);
    let r = ui.checkbox(&mut b, label);
    if r.changed() {
        value[key] = json!(b);
    }
    if !info.is_empty() {
        r.on_hover_text(info);
    }
}

impl Editor {
    pub fn properties(&mut self, ui: &mut egui::Ui) {
    let tone = colors(ui.ctx());
        ui.spacing_mut().scroll = egui::style::ScrollStyle {
            bar_width: 6.0,
            bar_inner_margin: 12.0,
            bar_outer_margin: 2.0,
            ..egui::style::ScrollStyle::solid()
        };
        if self.menus().is_empty() {
            return;
        }
        let path = self.selected_path();
        let Some(item) = self.node(&path).cloned() else { return };
        let kind = if path.is_empty() { "root" } else { item_type(&item) };
        let kind = kind.to_string();

        egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible).show(ui, |ui| {
            ui.add_space(14.0);
            ui.vertical_centered(|ui| {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(120.0), Sense::click());
                ui.painter().circle_filled(rect.center(), 60.0, if resp.hovered() { tone.line } else { tone.card });
                let theme = item["iconTheme"].as_str().unwrap_or("").to_string();
                let icon = item["icon"].as_str().unwrap_or("").to_string();
                self.icons.draw(ui.ctx(), ui.painter(), rect.shrink(18.0), &theme, &icon, tone.text);
                if resp.clicked() {
                    self.icon_theme = if icons::BUILTIN_THEMES.contains(&theme.as_str()) || icons::user_themes().contains(&theme) { theme } else { "material-symbols-rounded".into() };
                    self.icon_search.clear();
                    self.dialog = Dialog::Icon(path.clone());
                }

                let mut name = item["name"].as_str().unwrap_or("").to_string();
                let edit = egui::TextEdit::singleline(&mut name)
                    .font(FontId::new(24.0, FontFamily::Proportional))
                    .horizontal_align(egui::Align::Center)
                    .frame(false)
                    .desired_width(f32::INFINITY);
                if ui.add(edit).changed() {
                    if let Some(node) = self.node_mut(&path) {
                        node["name"] = json!(name);
                    }
                }
                let (line, _) = ui.allocate_exact_size(Vec2::new(ui.available_width() - 40.0, 14.0), Sense::hover());
                ui.painter().line_segment([line.left_center(), line.right_center()], Stroke::new(1.0, tone.line));
                ui.painter().circle_filled(line.center(), 5.0, tone.line);
            });
            ui.add_space(10.0);

            if kind == "root" {
                self.menu_settings(ui);
            }
            self.workflow_editor(ui, &kind, &path);
        });
    }

    fn menu_settings(&mut self, ui: &mut egui::Ui) {
    let tone = colors(ui.ctx());
        let index = self.menu;
        let mut menu = self.doc["menus"][index].clone();
        let recording = self.recording == Recording::Shortcut;

        row(ui, &self.t("settings.shortcut-label"), &self.t("settings.shortcut-info"), |ui| {
            let dot = self.icons.glyph("radio_button_checked");
            if round_button(ui, dot, 30.0, &self.tr.t("settings.shortcut-recording")).clicked() {
                self.recording = if recording { Recording::None } else { Recording::Shortcut };
            }
            let mut shortcut = menu["shortcut"].as_str().unwrap_or("").to_string();
            let hint = if recording { self.tr.t("settings.shortcut-recording") } else { self.tr.t("settings.not-bound") };
            if ui.add(egui::TextEdit::singleline(&mut shortcut).hint_text(hint).desired_width(f32::INFINITY)).changed() {
                menu["shortcut"] = json!(shortcut);
            }
        });

        row(ui, &self.t("settings.tags"), &self.t("settings.tags-info"), |ui| {
            let mut tags = menu["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
            if ui.add(egui::TextEdit::singleline(&mut tags).hint_text(self.tr.t("settings.add-tags-placeholder")).desired_width(f32::INFINITY)).changed() {
                menu["tags"] = Value::Array(tags.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()).map(|t| json!(t)).collect());
            }
        });
        ui.add_space(6.0);

        let arrows = [self.icons.glyph("chevron_right"), self.icons.glyph("expand_more")];
        section(ui, self.t("settings.menu-behavior"), arrows, |ui| {
            ui.label(RichText::new(self.t("settings.menu-behavior-info")).color(tone.dim).size(14.0));
            edit_bool(ui, &mut menu, "centered", &self.tr.t("settings.centered-mode"), &self.tr.t("settings.centered-mode-info"), false);
            edit_bool(ui, &mut menu, "anchored", &self.tr.t("settings.anchored-mode"), &self.tr.t("settings.anchored-mode-info"), false);
            edit_bool(ui, &mut menu, "hoverMode", &self.tr.t("settings.hover-mode"), &self.tr.t("settings.hover-mode-info"), false);
        });

        let mut pick = None;
        section(ui, self.t("settings.menu-conditions"), arrows, |ui| {
            ui.label(RichText::new(self.t("settings.menu-conditions-info")).color(tone.dim).size(14.0));
            if !menu["conditions"].is_object() {
                menu["conditions"] = json!({});
            }
            let conditions = &mut menu["conditions"];
            ui.label(self.tr.t("settings.app-condition")).on_hover_text(self.tr.t("settings.app-condition-info"));
            ui.horizontal(|ui| {
                if round_button(ui, self.icons.glyph("ads_click"), 30.0, &self.tr.t("settings.app-condition-tooltip")).clicked() {
                    pick = Some(Pick::AppCondition);
                }
                edit_text(ui, conditions, "appName", &self.tr.t("settings.app-condition-placeholder"), false);
            });
            ui.label(self.tr.t("settings.window-condition")).on_hover_text(self.tr.t("settings.window-condition-info"));
            ui.horizontal(|ui| {
                if round_button(ui, self.icons.glyph("ads_click"), 30.0, &self.tr.t("settings.window-condition-tooltip")).clicked() {
                    pick = Some(Pick::WindowCondition);
                }
                edit_text(ui, conditions, "windowName", &self.tr.t("settings.window-condition-placeholder"), false);
            });
            ui.label(self.tr.t("settings.area-condition")).on_hover_text(self.tr.t("settings.area-condition-info"));
            if !conditions["screenArea"].is_object() {
                conditions["screenArea"] = json!({});
            }
            ui.horizontal(|ui| {
                let width = (ui.available_width() - ui.spacing().item_spacing.x * 3.0) / 4.0;
                for (key, hint) in [
                    ("xMin", "settings.area-condition-left-placeholder"),
                    ("xMax", "settings.area-condition-right-placeholder"),
                    ("yMin", "settings.area-condition-top-placeholder"),
                    ("yMax", "settings.area-condition-bottom-placeholder"),
                ] {
                    let mut text = conditions["screenArea"][key].as_f64().map(|v| v.to_string()).unwrap_or_default();
                    if ui.add(egui::TextEdit::singleline(&mut text).hint_text(self.tr.t(hint)).desired_width(width)).changed() {
                        conditions["screenArea"][key] = text.trim().parse::<f64>().map(|v| json!(v)).unwrap_or(Value::Null);
                    }
                }
            });
        });
        ui.add_space(10.0);
        if let Some(p) = pick {
            self.picked = None;
            self.countdown = None;
            self.dialog = Dialog::Window(p);
        }

        if menu != self.doc["menus"][index] {
            self.doc["menus"][index] = menu;
        }
    }

    fn workflow_editor(&mut self, ui: &mut egui::Ui, kind: &str, path: &[usize]) {
    let tone = colors(ui.ctx());
        let flows = workflows(kind);
        if !flows.iter().any(|f| f.0 == self.workflow_tab) {
            self.workflow_tab = flows[0].0.to_string();
        }

        if flows.len() > 1 {
            ui.horizontal(|ui| {
                let width = flows.len() as f32 * 54.0;
                ui.add_space((ui.available_width() - width) / 2.0);
                for (key, short, _) in flows {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(46.0), Sense::click());
                    let active = self.workflow_tab == *key;
                    ui.painter().circle_filled(rect.center(), 23.0, if active { tone.card } else if resp.hovered() { tone.line } else { Color32::TRANSPARENT });
                    if active {
                        ui.painter().circle_stroke(rect.center(), 23.0, Stroke::new(1.5, tone.text));
                    }
                    self.icons.draw(ui.ctx(), ui.painter(), rect.shrink(9.0), "kando", &format!("workflow-{short}.svg"), tone.text);
                    let name = self.t(&format!("settings.workflow-editor.{short}-workflow-name"));
                    if resp.on_hover_text(name).clicked() {
                        self.workflow_tab = key.to_string();
                    }
                    ui.add_space(4.0);
                }
            });
            ui.add_space(6.0);
        }

        let (key, short, text_key) = *flows.iter().find(|f| f.0 == self.workflow_tab).unwrap();
        let item = self.node(path).cloned().unwrap_or(Value::Null);
        let mut flow = if item[key].is_object() { item[key].clone() } else { json!({ "actions": [] }) };
        if !flow["actions"].is_array() {
            flow["actions"] = json!([]);
        }

        egui::Frame::new().fill(tone.card).stroke(Stroke::new(1.0, tone.line)).corner_radius(CornerRadius::same(10)).inner_margin(14).show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                self.icons.draw(ui.ctx(), ui.painter(), rect, "kando", &format!("workflow-{short}.svg"), tone.text);
                ui.label(RichText::new(self.t(&format!("settings.workflow-editor.{short}-workflow-name"))).size(19.0).strong());
            });
            let recording = self.recording == Recording::QuickKey(key.to_string());
            row(ui, &self.t("settings.quick-select-key-label"), &self.t("settings.quick-select-key-info"), |ui| {
                let dot = self.icons.glyph("radio_button_checked");
                if round_button(ui, dot, 30.0, &self.tr.t("settings.quick-select-key-recording")).clicked() {
                    self.recording = if recording { Recording::None } else { Recording::QuickKey(key.to_string()) };
                }
                let mut quick = flow["quickSelectKey"].as_str().unwrap_or("").to_string();
                let hint = if recording { self.tr.t("settings.quick-select-key-recording") } else { String::new() };
                if ui.add(egui::TextEdit::singleline(&mut quick).hint_text(hint).desired_width(120.0)).changed() {
                    flow["quickSelectKey"] = json!(quick);
                }
            });
            ui.label(RichText::new(self.t(&format!("settings.workflow-editor.{text_key}.description"))).color(tone.dim).size(14.0));
        });

        let count = flow["actions"].as_array().map_or(0, |a| a.len());
        if count == 0 {
            ui.add_space(6.0);
            ui.vertical_centered(|ui| ui.label(RichText::new(self.t(&format!("settings.workflow-editor.{text_key}.empty-hint"))).color(tone.dim)));
        }
        let mut remove = None;
        let mut swap = None;
        for i in 0..count {
            ui.vertical_centered(|ui| {
                ui.painter().text(ui.cursor().min + Vec2::new(ui.available_width() / 2.0, 8.0), Align2::CENTER_CENTER, "▼", FontId::proportional(14.0), tone.line);
            });
            ui.add_space(14.0);
            let action = &mut flow["actions"][i];
            let t = action["type"].as_str().unwrap_or("").to_string();
            let meta = action_meta(&t);
            egui::Frame::new().fill(tone.card).stroke(Stroke::new(1.0, tone.line)).corner_radius(CornerRadius::same(10)).inner_margin(12).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                    self.icons.draw(ui.ctx(), ui.painter(), rect, "kando", meta.icon, tone.text);
                    ui.label(RichText::new(self.tr.t(&format!("menu-actions.{t}.name"))).size(18.0).strong())
                        .on_hover_text(self.tr.t(&format!("menu-actions.{t}.description")));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if round_button(ui, self.icons.glyph("delete"), 28.0, "").clicked() {
                            remove = Some(i);
                        }
                        if i + 1 < count && round_button(ui, self.icons.glyph("arrow_downward"), 28.0, "").clicked() {
                            swap = Some((i, i + 1));
                        }
                        if i > 0 && round_button(ui, self.icons.glyph("arrow_upward"), 28.0, "").clicked() {
                            swap = Some((i - 1, i));
                        }
                    });
                });
                self.action_config(ui, action, key, i);
            });
        }
        if let Some(i) = remove {
            flow["actions"].as_array_mut().unwrap().remove(i);
        }
        if let Some((a, b)) = swap {
            flow["actions"].as_array_mut().unwrap().swap(a, b);
        }

        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            let plus = self.icons.glyph("add");
            if crate::pill(ui, &self.t("settings.workflow-editor.add-action"), Some(plus)).clicked() {
                self.action_search.clear();
                self.dialog = Dialog::Action(key.to_string());
            }
        });
        ui.add_space(20.0);

        if flow != item[key] && !(item[key].is_null() && flow == json!({ "actions": [] })) {
            if let Some(node) = self.node_mut(path) {
                node[key] = flow;
            }
        }
    }

    fn action_config(&mut self, ui: &mut egui::Ui, action: &mut Value, workflow: &str, index: usize) {
    let tone = colors(ui.ctx());
        let t = action["type"].as_str().unwrap_or("").to_string();
        let tr = |k: &str| self.tr.t(k);
        match t.as_str() {
            "execute-command" => {
                if ui.button(tr("menu-actions.execute-command.choose-app")).clicked() {
                    self.app_search.clear();
                    self.app_choice = None;
                    self.dialog = Dialog::Apps(Pick::Command(workflow.to_string(), index));
                }
                edit_text(ui, action, "command", &tr("menu-actions.execute-command.placeholder"), true);
                edit_bool(ui, action, "detached", &tr("menu-actions.execute-command.detached"), &tr("menu-actions.execute-command.detached-info"), true);
                edit_bool(ui, action, "isolated", &tr("menu-actions.execute-command.isolated"), &tr("menu-actions.execute-command.isolated-info"), false);
            }
            "open-file" => {
                ui.horizontal(|ui| {
                    if round_button(ui, self.icons.glyph("folder_open"), 30.0, &tr("settings.file-picker.select-file")).clicked() {
                        if let Some(file) = rfd::FileDialog::new().pick_file() {
                            action["path"] = json!(file.to_string_lossy());
                        }
                    }
                    edit_text(ui, action, "path", &tr("menu-actions.open-file.placeholder"), false);
                });
            }
            "open-uri" => edit_text(ui, action, "uri", &tr("menu-actions.open-uri.placeholder"), false),
            "send-websocket-message" => {
                edit_text(ui, action, "url", "ws://localhost:8080", false);
                edit_text(ui, action, "message", &tr("menu-actions.send-websocket-message.placeholder"), true);
            }
            "set-clipboard" => edit_text(ui, action, "text", &tr("menu-actions.set-clipboard.placeholder"), true),
            "delay" => {
                let mut d = action["duration"].as_f64().unwrap_or(1.0);
                ui.horizontal(|ui| {
                    ui.label(tr("menu-actions.delay.duration-label")).on_hover_text(tr("menu-actions.delay.duration-info"));
                    if ui.add(egui::DragValue::new(&mut d).range(0.0..=3600.0).speed(0.05)).changed() {
                        action["duration"] = json!(d);
                    }
                });
            }
            "simulate-hotkey" => {
                let recording = self.recording == Recording::Hotkey(workflow.to_string(), index);
                ui.horizontal(|ui| {
                    if round_button(ui, self.icons.glyph("radio_button_checked"), 30.0, &tr("menu-actions.simulate-hotkey.recording-placeholder")).clicked() {
                        self.recording = if recording { Recording::None } else { Recording::Hotkey(workflow.to_string(), index) };
                    }
                    let hint = if recording { tr("menu-actions.simulate-hotkey.recording-placeholder") } else { tr("menu-actions.simulate-hotkey.hotkey") };
                    edit_text(ui, action, "hotkey", &hint, false);
                });
            }
            "execute-macro" => {
                let recording = self.recording == Recording::Macro(workflow.to_string(), index);
                let mut lines = action["macro"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|e| format!("{} {} {}", e["type"].as_str().unwrap_or(""), e["key"].as_str().unwrap_or(""), e["delay"].as_u64().unwrap_or(10)))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    let glyph = self.icons.glyph(if recording { "stop_circle" } else { "radio_button_checked" });
                    if round_button(ui, glyph, 30.0, &tr("menu-actions.execute-macro.recording-placeholder")).clicked() {
                        if recording {
                            self.recording = Recording::None;
                        } else {
                            action["macro"] = json!([]);
                            self.macro_mods = egui::Modifiers::NONE;
                            self.recording = Recording::Macro(workflow.to_string(), index);
                        }
                    }
                    if recording {
                        ui.label(RichText::new(tr("menu-actions.execute-macro.recording-placeholder")).color(tone.text));
                    }
                });
                let hint = tr("menu-actions.execute-macro.placeholder");
                if ui.add(egui::TextEdit::multiline(&mut lines).hint_text(hint).font(egui::TextStyle::Monospace).desired_rows(4).desired_width(f32::INFINITY)).changed() {
                    let events: Vec<Value> = lines
                        .lines()
                        .filter_map(|l| {
                            let mut parts = l.split_whitespace();
                            let kind = parts.next()?;
                            let key = parts.next()?;
                            let delay = parts.next().and_then(|d| d.parse::<u64>().ok()).unwrap_or(10);
                            Some(json!({ "type": kind, "key": key, "delay": delay }))
                        })
                        .collect();
                    action["macro"] = Value::Array(events);
                }
            }
            "focus-window" => {
                ui.label(RichText::new(tr("menu-actions.focus-window.hint")).color(tone.dim).size(14.0));
                ui.horizontal(|ui| {
                    if round_button(ui, self.icons.glyph("ads_click"), 30.0, &tr("settings.app-condition-tooltip")).clicked() {
                        self.picked = None;
                        self.countdown = None;
                        self.dialog = Dialog::Window(Pick::FocusApp(workflow.to_string(), index));
                    }
                    edit_text(ui, action, "appName", &tr("settings.app-condition-placeholder"), false);
                });
                ui.horizontal(|ui| {
                    if round_button(ui, self.icons.glyph("ads_click"), 30.0, &tr("settings.window-condition-tooltip")).clicked() {
                        self.picked = None;
                        self.countdown = None;
                        self.dialog = Dialog::Window(Pick::FocusWindow(workflow.to_string(), index));
                    }
                    edit_text(ui, action, "windowName", &tr("settings.window-condition-placeholder"), false);
                });
            }
            "open-menu" => {
                let names: Vec<String> = self.menus().iter().filter_map(|m| m["root"]["name"].as_str().map(String::from)).collect();
                let mut current = action["menu"].as_str().unwrap_or("").to_string();
                egui::ComboBox::from_id_salt(("open-menu", workflow, index))
                    .width(ui.available_width())
                    .selected_text(if current.is_empty() { tr("settings.invalid-dropdown-selection") } else { current.clone() })
                    .show_ui(ui, |ui| {
                        for n in &names {
                            ui.selectable_value(&mut current, n.clone(), n);
                        }
                    });
                if action["menu"].as_str() != Some(current.as_str()) {
                    action["menu"] = json!(current);
                }
            }
            _ => {
                ui.label(RichText::new(tr(&format!("menu-actions.{t}.description"))).color(tone.dim).size(14.0));
            }
        }
    }

    pub fn add_action(&mut self, workflow: &str, kind: &str) {
        let path = self.selected_path();
        let action = action_meta(kind).create();
        if let Some(node) = self.node_mut(&path) {
            if !node[workflow].is_object() {
                node[workflow] = json!({ "actions": [] });
            }
            if !node[workflow]["actions"].is_array() {
                node[workflow]["actions"] = json!([]);
            }
            node[workflow]["actions"].as_array_mut().unwrap().push(action);
        }
    }
}
