use crate::style::colors;
use base64::{engine::general_purpose::STANDARD, Engine};
use eframe::egui::{self, Color32, RichText, Sense, Vec2};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

use crate::props::{action_meta, ACTIONS};
use crate::{apps, icons, open_url, sound, theme, Dialog, Editor, Pick};

const PREVIEWS: &[(&str, &[u8])] = &[
    ("default", include_bytes!("../../assets/previews/default.webp")),
    ("clean-circle", include_bytes!("../../assets/previews/clean-circle.webp")),
    ("neon-lights", include_bytes!("../../assets/previews/neon-lights.webp")),
    ("purity", include_bytes!("../../assets/previews/purity.webp")),
    ("rainbow-labels", include_bytes!("../../assets/previews/rainbow-labels.webp")),
];

const TUTORIAL: &[(&str, &str)] = &[
    ("tutorial-1", include_str!("../../assets/tutorial/tutorial-1.svg")),
    ("tutorial-2", include_str!("../../assets/tutorial/tutorial-2.svg")),
];

fn to_color32(c: theme::Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, (c[3] * 255.0) as u8)
}

fn css(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a == 255 {
        format!("rgb({r} {g} {b})")
    } else {
        format!("rgb({r} {g} {b} / {:.2})", a as f32 / 255.0)
    }
}

fn field_row(ui: &mut egui::Ui, label: String, info: String, add: impl FnOnce(&mut egui::Ui, f32)) {
    let width = ui.available_width();
    let field = (width * 0.44).floor();
    let label_width = width - field - ui.spacing().item_spacing.x;
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(Vec2::new(label_width, 36.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_width(label_width);
            ui.add(egui::Label::new(label).wrap()).on_hover_text(info);
        });
        ui.allocate_ui_with_layout(Vec2::new(field, 36.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_width(field);
            add(ui, field);
        });
    });
}

fn bool_row(ui: &mut egui::Ui, config: &mut Value, key: &str, label: String, info: String, default: bool) {
    field_row(ui, label, info, |ui, _| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut b = config[key].as_bool().unwrap_or(default);
            if ui.checkbox(&mut b, "").changed() {
                config[key] = json!(b);
            }
        });
    });
}

fn number_row(ui: &mut egui::Ui, config: &mut Value, key: &str, label: String, info: String, default: f64, range: std::ops::RangeInclusive<f64>, suffix: &str) {
    field_row(ui, label, info, |ui, width| {
        let mut v = config[key].as_f64().unwrap_or(default);
        let speed = if *range.end() <= 4.0 { 0.01 } else { 1.0 };
        if ui.add_sized([width, 36.0], egui::DragValue::new(&mut v).range(range).speed(speed).suffix(suffix)).changed() {
            config[key] = json!(v);
        }
    });
}

fn choice_row(ui: &mut egui::Ui, config: &mut Value, key: &str, label: String, info: String, default: &str, options: &[(String, String)]) {
    field_row(ui, label, info, |ui, width| {
        let mut current = config[key].as_str().unwrap_or(default).to_string();
        let shown = options.iter().find(|o| o.0 == current).map(|o| o.1.clone()).unwrap_or_else(|| current.clone());
        egui::ComboBox::from_id_salt(key).selected_text(shown).width(width).truncate().show_ui(ui, |ui| {
            for (value, text) in options {
                ui.selectable_value(&mut current, value.clone(), text);
            }
        });
        if config[key].as_str() != Some(current.as_str()) {
            config[key] = json!(current);
        }
    });
}

fn heading(ui: &mut egui::Ui, text: String) {
    ui.add_space(16.0);
    ui.label(RichText::new(text).size(20.0));
    ui.separator();
    ui.add_space(4.0);
}

fn window(ctx: &egui::Context, title: String, size: [f32; 2]) -> egui::Window<'static> {
    egui::Window::new(title)
        .collapsible(false)
        .default_size(size)
        .max_height((ctx.screen_rect().height() - 100.0).max(320.0))
        .frame(egui::Frame::window(&ctx.style()).inner_margin(8))
        .default_pos(ctx.screen_rect().center() - Vec2::from(size) / 2.0)
        .movable(true)
        .order(egui::Order::Foreground)
}

fn backdrop(ctx: &egui::Context) -> egui::Response {
    let area = egui::Area::new(egui::Id::new("dialog-backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(ctx.screen_rect().min)
        .movable(false);
    ctx.memory_mut(|m| m.set_modal_layer(area.layer()));
    area.show(ctx, |ui| {
        ui.allocate_exact_size(ctx.screen_rect().size(), Sense::click_and_drag()).1
    }).inner
}

fn body(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().inner_margin(16).show(ui, add);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_and_dismiss() {
        let ctx = egui::Context::default();
        crate::style::apply(&ctx, true);
        let run = |events| {
            let mut rect = egui::Rect::NOTHING;
            let mut outside = false;
            let mut behind = false;
            let _ = ctx.run(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1480.0, 900.0))),
                events,
                ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    behind = ui.add_sized([120.0, 40.0], egui::Button::new("Orbit Actions")).clicked();
                });
                outside = backdrop(ctx).clicked();
                rect = window(ctx, "Orbit Actions".into(), [600.0, 480.0]).show(ctx, |ui| {
                    ui.label("Orbit Actions");
                }).unwrap().response.rect;
            });
            (rect, outside, behind)
        };
        for _ in 0..3 {
            run(vec![]);
        }
        let (rect, _, _) = run(vec![]);
        let start = rect.center_top() + Vec2::new(0.0, 18.0);
        let end = start + Vec2::new(80.0, 60.0);
        let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        run(vec![egui::Event::PointerMoved(start)]);
        run(vec![button(start, true)]);
        run(vec![egui::Event::PointerMoved(end)]);
        run(vec![button(end, false)]);
        let (moved, _, _) = run(vec![]);
        assert!((moved.min - rect.min - Vec2::new(80.0, 60.0)).length() < 2.0);
        let pos = egui::pos2(24.0, 24.0);
        run(vec![egui::Event::PointerMoved(pos)]);
        run(vec![button(pos, true)]);
        let (_, outside, behind) = run(vec![button(pos, false)]);
        assert!(outside);
        assert!(!behind);
    }

    #[test]
    fn close_dialog() {
        for dark in [false, true] {
            for size in [[600.0, 480.0], [860.0, 740.0], [1000.0, 600.0]] {
                let ctx = egui::Context::default();
                crate::style::apply(&ctx, dark);
                let mut open = true;
                let mut rect = egui::Rect::NOTHING;
                let mut run = |events| {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1480.0, 900.0))),
                        events,
                        ..Default::default()
                    };
                    let _ = ctx.run(input, |ctx| {
                        if let Some(response) = window(ctx, "Orbit Actions".into(), size).open(&mut open).show(ctx, |ui| body(ui, |ui| {
                            ui.label("Orbit Actions");
                        })) {
                            rect = response.response.rect;
                        }
                    });
                    (open, rect)
                };
                for _ in 0..3 {
                    run(vec![]);
                }
                let (_, rect) = run(vec![]);
                let height = ctx.fonts(|f| f.row_height(&egui::FontId::proportional(22.0))) + 16.0;
                let pos = egui::pos2(rect.right() - 1.0 - height / 2.0, rect.top() + 1.0 + height / 2.0);
                run(vec![egui::Event::PointerMoved(pos)]);
                run(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
                let (open, _) = run(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
                assert!(!open, "{dark} {size:?}");
            }
        }
    }
}

pub fn show(ed: &mut Editor, ctx: &egui::Context) {
    let dialog = ed.dialog.clone();
    if matches!(dialog, Dialog::None) {
        return;
    }
    if !matches!(dialog, Dialog::None)
        && ed.recording == crate::Recording::None
        && !ctx.memory(|m| m.any_popup_open())
        && ctx.input(|i| i.key_pressed(egui::Key::Escape))
    {
        ed.dialog = Dialog::None;
        return;
    }
    let popup = ctx.memory(|m| m.any_popup_open());
    let outside = backdrop(ctx);
    let mut open = true;
    match dialog {
        Dialog::None => return,
        Dialog::Settings => {
            window(ctx, ed.t("settings.general-settings-dialog.title"), [860.0, 740.0]).open(&mut open).show(ctx, |ui| body(ui, |ui| general(ed, ui)));
        }
        Dialog::Themes => {
            window(ctx, ed.t("settings.menu-themes-dialog.title"), [860.0, 760.0]).open(&mut open).show(ctx, |ui| body(ui, |ui| themes(ed, ui)));
        }
        Dialog::About => {
            window(ctx, ed.t("settings.about-dialog.title"), [600.0, 480.0]).open(&mut open).resizable(false).show(ctx, |ui| body(ui, |ui| about(ed, ui)));
        }
        Dialog::Intro => {
            window(ctx, ed.t("settings.introduction-dialog.title"), [1000.0, 600.0]).open(&mut open).show(ctx, |ui| body(ui, |ui| intro(ed, ui)));
        }
        Dialog::Apps(pick) => {
            let mut done = false;
            window(ctx, ed.t("settings.app-picker.search-placeholder"), [520.0, 600.0]).open(&mut open).show(ctx, |ui| body(ui, |ui| done = app_picker(ed, ui, &pick)));
            if done {
                open = false;
            }
        }
        Dialog::Window(pick) => {
            let title = if matches!(pick, Pick::AppCondition | Pick::FocusApp(..)) {
                ed.t("settings.window-picker-dialog.pick-app-name-title")
            } else {
                ed.t("settings.window-picker-dialog.pick-window-title-title")
            };
            let mut done = false;
            window(ctx, title, [480.0, 260.0]).open(&mut open).resizable(false).show(ctx, |ui| body(ui, |ui| done = window_picker(ed, ui, &pick)));
            if done {
                open = false;
            }
        }
        Dialog::Icon(path) => {
            let mut chosen = None;
            window(ctx, ed.t("settings.icon-picker-dialog.search-placeholder"), [640.0, 580.0]).open(&mut open).show(ctx, |ui| body(ui, |ui| chosen = icon_picker(ed, ui)));
            if let Some((theme, icon)) = chosen {
                if let Some(node) = ed.node_mut(&path) {
                    node["iconTheme"] = json!(theme);
                    node["icon"] = json!(icon);
                }
                if !matches!(ed.icon_theme.as_str(), "emoji" | "base64") {
                    open = false;
                }
            }
        }
        Dialog::CollectionIcon(index) => {
            let mut chosen = None;
            window(ctx, ed.t("settings.icon-picker-dialog.search-placeholder"), [640.0, 580.0]).open(&mut open).show(ctx, |ui| body(ui, |ui| chosen = icon_picker(ed, ui)));
            if let Some((theme, icon)) = chosen {
                ed.doc["collections"][index]["iconTheme"] = json!(theme);
                ed.doc["collections"][index]["icon"] = json!(icon);
                if !matches!(ed.icon_theme.as_str(), "emoji" | "base64") {
                    open = false;
                }
            }
        }
        Dialog::Action(workflow) => {
            let mut picked = None;
            window(ctx, ed.t("settings.workflow-editor.add-action"), [460.0, 520.0]).open(&mut open).show(ctx, |ui| body(ui, |ui| picked = action_picker(ed, ui)));
            if let Some(kind) = picked {
                ed.add_action(&workflow, kind);
                open = false;
            }
        }
    }
    if !open || (!popup && outside.clicked()) {
        ed.dialog = Dialog::None;
    }
}

fn general(ed: &mut Editor, ui: &mut egui::Ui) {
    crate::style::dialog(ui);
    let tone = colors(ui.ctx());
    let t = |k: &str| ed.tr.t(&format!("settings.general-settings-dialog.{k}"));
    let mut c = ed.config.clone();
    let mut restored = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label(RichText::new(t("message")).color(tone.dim));

        heading(ui, t("app-settings"));
        let mut languages = vec![("auto".to_string(), t("auto-language"))];
        for (code, _) in crate::tr::LOCALES {
            let name = LANGUAGES.iter().find(|(c, _)| c == code).map_or(*code, |(_, n)| *n);
            languages.push((code.to_string(), name.to_string()));
        }
        choice_row(ui, &mut c, "locale", t("localization-label"), t("localization-info"), "auto", &languages);
        choice_row(ui, &mut c, "trayIconFlavor", t("tray-icon-flavor"), t("tray-icon-flavor-info"), "color", &[
            ("color".into(), t("color")),
            ("white".into(), t("white")),
            ("black".into(), t("black")),
            ("none".into(), t("hidden")),
        ]);
        choice_row(ui, &mut c, "settingsWindowColorScheme", t("settings-window-color-scheme"), t("settings-window-color-scheme-info"), "system", &[
            ("system".into(), t("system")),
            ("light".into(), t("light")),
            ("dark".into(), t("dark")),
        ]);
        bool_row(ui, &mut c, "enableVersionCheck", t("check-for-new-versions"), t("check-for-new-versions-info"), true);

        heading(ui, t("settings-dialog-and-tray-icon"));
        bool_row(ui, &mut c, "hideSettingsButton", t("invisible-settings-button"), t("invisible-settings-button-info"), false);
        choice_row(ui, &mut c, "settingsButtonPosition", t("settings-button-position"), t("settings-button-position-info"), "bottom-right", &[
            ("top-left".into(), t("top-left")),
            ("top-right".into(), t("top-right")),
            ("bottom-left".into(), t("bottom-left")),
            ("bottom-right".into(), t("bottom-right")),
        ]);

        heading(ui, t("interaction-modes"));
        bool_row(ui, &mut c, "enableMarkingMode", t("enable-marking-mode"), t("enable-marking-mode-info"), true);
        bool_row(ui, &mut c, "enableTurboMode", t("enable-turbo-mode"), t("enable-turbo-mode-info"), true);
        bool_row(ui, &mut c, "hoverModeNeedsConfirmation", t("require-click-for-hover-mode"), t("require-click-for-hover-mode-info"), false);
        bool_row(ui, &mut c, "enableGamepad", t("enable-gamepad-support"), t("enable-gamepad-support-info"), true);

        heading(ui, t("menu-behavior"));
        bool_row(ui, &mut c, "keepInputFocus", t("keep-input-focus"), t("keep-input-focus-info"), false);
        bool_row(ui, &mut c, "rmbSelectsParent", t("right-mouse-button-selects-parent"), t("right-mouse-button-selects-parent-info"), false);
        bool_row(ui, &mut c, "warpMouse", t("move-pointer-to-menu-center"), t("move-pointer-to-menu-center-info"), true);
        bool_row(ui, &mut c, "returnPointerToMenuOpeningPosition", t("return-pointer-after-selection"), t("return-pointer-after-selection-info"), false);
        bool_row(ui, &mut c, "triggerCenterClickOnKeyRelease", t("trigger-center-click-on-key-release"), t("trigger-center-click-on-key-release-info"), false);
        choice_row(ui, &mut c, "sameShortcutBehavior", t("press-again-behavior"), t("press-again-behavior-info"), "nothing", &[
            ("nothing".into(), t("do-nothing")),
            ("cycle-from-first".into(), t("cycle-from-first")),
            ("cycle-from-recent".into(), t("cycle-from-recent")),
            ("close".into(), t("close-menu")),
        ]);

        heading(ui, t("menu-sounds"));
        let mut sounds = vec![("none".to_string(), t("none"))];
        sounds.extend(sound::installed());
        choice_row(ui, &mut c, "soundTheme", t("sound-theme"), t("sound-theme-info"), "none", &sounds);
        number_row(ui, &mut c, "soundVolume", t("volume"), t("volume-info"), 0.5, 0.0..=1.0, "");
        ui.label(RichText::new(t("learn-how-to-add-sound-themes")).color(tone.dim));

        heading(ui, t("advanced-menu-options"));
        ui.label(RichText::new(t("advanced-menu-options-note")).color(tone.dim));
        number_row(ui, &mut c, "centerDeadZone", t("center-click-zone-radius"), t("center-click-zone-radius-info"), 50.0, 0.0..=500.0, " px");
        number_row(ui, &mut c, "minParentDistance", t("minimum-submenu-distance"), t("minimum-submenu-distance-info"), 150.0, 0.0..=1000.0, " px");
        number_row(ui, &mut c, "dragThreshold", t("movement-threshold"), t("movement-threshold-info"), 15.0, 0.0..=200.0, " px");
        number_row(ui, &mut c, "maxSelectionRadius", t("max-selection-radius"), t("max-selection-radius-info"), 0.0, 0.0..=2000.0, " px");
        number_row(ui, &mut c, "gestureMinStrokeLength", t("minimum-gesture-length"), t("minimum-gesture-length-info"), 150.0, 0.0..=1000.0, " px");
        number_row(ui, &mut c, "gestureMinStrokeAngle", t("minimum-gesture-angle"), t("minimum-gesture-angle-info"), 20.0, 0.0..=90.0, "°");
        number_row(ui, &mut c, "gestureJitterThreshold", t("gesture-jitter-threshold"), t("gesture-jitter-threshold-info"), 10.0, 0.0..=100.0, " px");
        number_row(ui, &mut c, "gesturePauseTimeout", t("gesture-pause-timeout"), t("gesture-pause-timeout-info"), 100.0, 0.0..=2000.0, " ms");
        number_row(ui, &mut c, "fixedStrokeLength", t("fixed-stroke-length"), t("fixed-stroke-length-info"), 0.0, 0.0..=1000.0, " px");

        heading(ui, t("backup-and-restore"));
        ui.horizontal_wrapped(|ui| {
            let menus_path = crate::data_dir().join("menus.json");
            let config_path = crate::data_dir().join("config.json");
            if ui.button(t("backup-menus")).on_hover_text(t("backup-menus-note")).clicked() {
                if let Some(f) = rfd::FileDialog::new().set_file_name("menus.json").save_file() {
                    let _ = std::fs::copy(&menus_path, f);
                }
            }
            if ui.button(t("backup-settings")).on_hover_text(t("backup-settings-note")).clicked() {
                if let Some(f) = rfd::FileDialog::new().set_file_name("config.json").save_file() {
                    let _ = std::fs::copy(&config_path, f);
                }
            }
            if ui.button(t("restore-menus")).clicked() {
                if let Some(f) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file() {
                    if let Ok(v) = serde_json::from_str::<Value>(&std::fs::read_to_string(f).unwrap_or_default()) {
                        if v["menus"].is_array() {
                            restored = Some(v);
                        }
                    }
                }
            }
            if ui.button(t("restore-settings")).clicked() {
                if let Some(f) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file() {
                    if let Ok(v) = serde_json::from_str::<Value>(&std::fs::read_to_string(f).unwrap_or_default()) {
                        if v.is_object() {
                            c = v;
                        }
                    }
                }
            }
        });
    });
    if let Some(v) = restored {
        ed.doc = v;
        ed.fix_selection();
    }
    if c != ed.config {
        ed.config = c;
    }
}

fn themes(ed: &mut Editor, ui: &mut egui::Ui) {
    let tone = colors(ui.ctx());
    let t = |k: &str| ed.tr.t(&format!("settings.menu-themes-dialog.{k}"));
    let mut c = ed.config.clone();
    egui::ScrollArea::vertical().show(ui, |ui| {
        bool_row(ui, &mut c, "enableDarkModeForMenuThemes", t("light-dark-mode"), t("light-dark-mode-info"), false);
        let dual = c["enableDarkModeForMenuThemes"].as_bool().unwrap_or(false);
        if dual {
            ui.horizontal(|ui| {
                if ui.selectable_label(!ed.dark_variant, t("light")).clicked() {
                    ed.dark_variant = false;
                }
                if ui.selectable_label(ed.dark_variant, t("dark")).clicked() {
                    ed.dark_variant = true;
                }
            });
        } else {
            ed.dark_variant = false;
        }
        let (theme_key, colors_key) = if ed.dark_variant { ("darkMenuTheme", "darkMenuThemeColors") } else { ("menuTheme", "menuThemeColors") };
        let current = c[theme_key].as_str().unwrap_or("default").to_string();

        ui.add_space(6.0);
        let card = 150.0;
        ui.horizontal_wrapped(|ui| {
            for (id, name, author) in theme::THEMES {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(card, card + 44.0), Sense::click());
                let active = current == *id;
                ui.painter().rect_filled(rect, 10, if active || resp.hovered() { tone.hover } else { tone.card });
                if let Some(art) = PREVIEWS.iter().find(|p| p.0 == *id).map(|p| p.1) {
                    if let Some(tex) = ed.icons.image(ui.ctx(), id, art) {
                        let img = egui::Rect::from_min_size(rect.min + Vec2::new(6.0, 6.0), Vec2::splat(card - 12.0));
                        ui.painter().image(tex.id(), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    }
                }
                ui.painter().text(egui::pos2(rect.center().x, rect.bottom() - 30.0), egui::Align2::CENTER_CENTER, *name, egui::FontId::proportional(15.0), tone.text);
                let by = ed.tr.t("settings.menu-themes-dialog.author").replace("{{author}}", author);
                ui.painter().text(egui::pos2(rect.center().x, rect.bottom() - 12.0), egui::Align2::CENTER_CENTER, by, egui::FontId::proportional(11.0), tone.dim);
                if resp.clicked() && !active {
                    c[theme_key] = json!(id);
                }
            }
        });

        heading(ui, t("theme-colors"));
        let id = c[theme_key].as_str().unwrap_or("default").to_string();
        if !c[colors_key].is_object() {
            c[colors_key] = json!({});
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("presets").selected_text(t("choose-preset")).show_ui(ui, |ui| {
                for (name, text) in theme::presets(&id) {
                    if ui.selectable_label(false, *name).clicked() {
                        if let Ok(v) = serde_json::from_str::<Value>(text) {
                            c[colors_key][id.as_str()] = v["colors"].clone();
                        }
                    }
                }
            });
            if ui.button(t("reset-color-picker")).clicked() {
                c[colors_key][id.as_str()] = json!({});
            }
        });
        if !c[colors_key][id.as_str()].is_object() {
            c[colors_key][id.as_str()] = json!({});
        }
        egui::Grid::new("colors").num_columns(2).spacing([24.0, 10.0]).show(ui, |ui| {
            for (key, default) in theme::default_colors(&id) {
                ui.label(*key);
                let current = c[colors_key][id.as_str()][key].as_str().unwrap_or(default).to_string();
                let mut color = theme::color(&current).map(to_color32).unwrap_or(tone.text);
                if egui::color_picker::color_edit_button_srgba(ui, &mut color, egui::color_picker::Alpha::OnlyBlend).changed() {
                    c[colors_key][id.as_str()][key] = json!(css(color));
                }
                ui.end_row();
            }
        });

        heading(ui, ed.tr.t("settings.menu-behavior"));
        number_row(ui, &mut c, "zoomFactor", t("menu-scale"), t("menu-scale-info"), 1.0, 0.25..=4.0, "");
        number_row(ui, &mut c, "fadeInDuration", t("fade-in-time"), t("fade-in-time-info"), 75.0, 0.0..=2000.0, " ms");
        number_row(ui, &mut c, "fadeOutDuration", t("fade-out-time"), t("fade-out-time-info"), 100.0, 0.0..=2000.0, " ms");
        bool_row(ui, &mut c, "enableMenuAnimations", t("enable-menu-animations"), t("enable-menu-animations-info"), true);
        bool_row(ui, &mut c, "enablePointerReactiveEffects", t("enable-pointer-reactive-effects"), t("enable-pointer-reactive-effects-info"), true);
        bool_row(ui, &mut c, "enableSelectionWedges", t("selection-wedges"), t("selection-wedges-info"), false);
        bool_row(ui, &mut c, "drawQuickSelectKey", t("draw-quick-select-key"), t("draw-quick-select-key-info"), false);
        bool_row(ui, &mut c, "underlineQuickSelectKey", t("underline-quick-select-key"), t("underline-quick-select-key-info"), true);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(t("open-theme-directory")).clicked() {
                let dir = crate::data_dir().join("menu-themes");
                let _ = std::fs::create_dir_all(&dir);
                open_url(&dir.to_string_lossy());
            }
        });
    });
    if c != ed.config {
        ed.config = c;
    }
}

fn about(ed: &mut Editor, ui: &mut egui::Ui) {
    let tone = colors(ui.ctx());
    crate::style::dialog(ui);
    let t = |k: &str| ed.tr.t(&format!("settings.about-dialog.{k}"));
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(88.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 44.0, tone.card);
        let logo = rect.shrink(16.0);
        let px = (logo.width() * ui.ctx().pixels_per_point()).round() as u32;
        if let Some(tex) = ed.icons.svg_texture(ui.ctx(), "star", crate::STAR, px) {
            ui.painter().image(tex.id(), logo, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tone.text);
        }
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.label(RichText::new("Orbit Actions").size(28.0));
            egui::Grid::new("about").num_columns(2).spacing([24.0, 8.0]).show(ui, |ui| {
                ui.label(RichText::new(t("kando-version")).color(tone.dim));
                ui.label(env!("CARGO_PKG_VERSION"));
                ui.end_row();
                ui.label(RichText::new(t("kando-backend")).color(tone.dim));
                ui.label("Win32 / Rust");
                ui.end_row();
            });
        });
    });
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(RichText::new(t("message")).color(tone.dim));
    ui.add_space(6.0);
    if ui.link(crate::REPOSITORY.trim_start_matches("https://")).clicked() {
        open_url(crate::REPOSITORY);
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(ed.tr.t("settings.introduction-dialog.based-on")).color(tone.dim));
        if ui.link("github.com/kando-menu/kando").clicked() {
            open_url("https://github.com/kando-menu/kando");
        }
    });
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(4.0);
    ui.columns(2, |cols| {
        if cols[0].add_sized([cols[0].available_width(), 40.0], egui::Button::new(t("read-release-notes"))).clicked() {
            open_url(&format!("{}/releases", crate::REPOSITORY));
        }
        if cols[1].add_sized([cols[1].available_width(), 40.0], egui::Button::new(t("check-latest-release"))).clicked() {
            open_url(&format!("{}/releases/latest", crate::REPOSITORY));
        }
    });
}

fn intro(ed: &mut Editor, ui: &mut egui::Ui) {
    let tone = colors(ui.ctx());
    crate::style::dialog(ui);
    let t = |k: &str| ed.tr.t(&format!("settings.introduction-dialog.{k}"));
    let chapters = [("chapter1", 0usize, 5usize), ("chapter2", 6, 7), ("chapter3", 8, 10), ("chapter4", 11, 11), ("chapter5", 12, 12)];
    let slides: Vec<(Option<String>, String, Option<&str>)> = vec![
        (Some(t("slide1-title")), t("slide1-text"), None),
        (Some(t("slide2-title")), t("slide2-text-standard"), None),
        (None, format!("{}\n\n• {}\n• {}", t("slide3-text"), t("slide3-hint1"), t("slide3-hint2")), Some("tutorial-1")),
        (None, t("slide4-text"), None),
        (None, format!("{}\n\n• {}", t("slide5-text"), t("slide5-hint")), Some("tutorial-2")),
        (None, t("slide6-text"), None),
        (Some(t("slide7-title")), t("slide7-text"), None),
        (None, t("slide8-text"), None),
        (Some(t("slide9-title")), t("slide9-text"), None),
        (None, t("slide10-text"), None),
        (None, t("slide11-text"), None),
        (Some(t("slide12-title")), t("slide12-text"), None),
        (None, t("slide13-text"), None),
    ];
    let mut show_again = ed.config["showIntroductionDialog"].as_bool().unwrap_or(true);
    let chapters: Vec<(String, usize, usize)> = chapters.iter().map(|(k, a, b)| (t(k), *a, *b)).collect();
    let again = t("show-again");
    let height = (ui.ctx().screen_rect().height() - 220.0).clamp(320.0, 480.0);
    ui.horizontal_top(|ui| {
        egui::Frame::new().fill(tone.card).corner_radius(10).inner_margin(16).show(ui, |ui| {
            ui.vertical(|ui| {
            ui.set_width(200.0);
            for (label, from, to) in &chapters {
                let active = ed.slide >= *from && ed.slide <= *to;
                if ui.add_sized([200.0, 40.0], egui::Button::new(RichText::new(label).size(16.0)).selected(active).frame(active)).clicked() {
                    ed.slide = *from;
                }
            }
            ui.add_space(24.0);
            if ui.checkbox(&mut show_again, again).changed() {
                ed.config["showIntroductionDialog"] = json!(show_again);
            }
            });
        });
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical().id_salt("intro-content").max_height(height - 64.0).auto_shrink([false, false]).show(ui, |ui| {
            let (title, text, image) = &slides[ed.slide.min(slides.len() - 1)];
            if let Some(title) = title {
                ui.label(RichText::new(title).size(26.0));
                ui.add_space(12.0);
            }
            if let Some(name) = image {
                if let Some(svg) = TUTORIAL.iter().find(|s| s.0 == *name).map(|s| s.1) {
                    if let Some(tex) = ed.icons.svg_texture(ui.ctx(), name, svg, 420) {
                        ui.add(egui::Image::new(&tex).fit_to_exact_size(Vec2::new(300.0, 220.0)));
                    }
                }
            }
            ui.label(RichText::new(text).size(17.0));
            if ed.slide == 0 {
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(ed.tr.t("settings.introduction-dialog.based-on")).color(tone.dim));
                    if ui.link("github.com/kando-menu/kando").clicked() {
                        open_url("https://github.com/kando-menu/kando");
                    }
                });
            }
            });
            ui.add_space(12.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ed.slide > 0 && crate::round_button(ui, ed.icons.glyph("chevron_left"), 36.0, "").clicked() {
                    ed.slide -= 1;
                }
                ui.label(format!("{} / {}", ed.slide + 1, slides.len()));
                if ed.slide + 1 < slides.len() && crate::round_button(ui, ed.icons.glyph("chevron_right"), 36.0, "").clicked() {
                    ed.slide += 1;
                }
            });
        });
    });
}

fn selected_action<'a>(ed: &'a mut Editor, workflow: &str, index: usize) -> Option<&'a mut Value> {
    let path = ed.selected_path();
    ed.node_mut(&path)?.get_mut(workflow)?.get_mut("actions")?.get_mut(index)
}

fn app_picker(ed: &mut Editor, ui: &mut egui::Ui, pick: &Pick) -> bool {
    let tone = colors(ui.ctx());
    ui.label(RichText::new(ed.t("settings.app-picker.hint")).color(tone.dim));
    let hint = ed.t("settings.app-picker.search-placeholder");
    ui.add(egui::TextEdit::singleline(&mut ed.app_search).hint_text(hint).desired_width(f32::INFINITY));
    let filter = ed.app_search.to_lowercase();
    let list: Vec<apps::App> = ed.icons.apps.iter().filter(|a| a.name.to_lowercase().contains(&filter)).cloned().collect();
    let row = 40.0;
    egui::ScrollArea::vertical().max_height(400.0).auto_shrink([false, false]).show_rows(ui, row, list.len(), |ui, range| {
        for app in &list[range] {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row - 4.0), Sense::click());
            let chosen = ed.app_choice.as_deref() == Some(app.id.as_str());
            if chosen || resp.hovered() {
                ui.painter().rect_filled(rect, 6, if chosen { tone.hover } else { tone.card });
            }
            let icon = egui::Rect::from_center_size(egui::pos2(rect.left() + 22.0, rect.center().y), Vec2::splat(30.0));
            ed.icons.draw(ui.ctx(), ui.painter(), icon, "system", &app.name, tone.text);
            ui.painter().text(egui::pos2(rect.left() + 46.0, rect.center().y), egui::Align2::LEFT_CENTER, &app.name, egui::FontId::proportional(15.0), tone.text);
            if resp.clicked() {
                ed.app_choice = Some(app.id.clone());
            }
        }
    });
    let use_it = ui.add_enabled(ed.app_choice.is_some(), egui::Button::new(ed.t("settings.app-picker.use-selected"))).clicked();
    if use_it {
        let id = ed.app_choice.clone().unwrap_or_default();
        if let Pick::Command(workflow, index) = pick {
            if let Some(action) = selected_action(ed, workflow, *index) {
                action["command"] = json!(format!("start \"\" \"shell:AppsFolder\\{id}\""));
            }
        }
        return true;
    }
    false
}

fn foreground() -> (String, String) {
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        let mut name = String::new();
        if !process.is_null() {
            let mut path = [0u16; 1024];
            let mut len = path.len() as u32;
            QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut len);
            let full = String::from_utf16_lossy(&path[..len as usize]);
            let file = full.rsplit('\\').next().unwrap_or("").to_string();
            name = file.rsplit_once('.').map(|(a, _)| a.to_string()).unwrap_or(file);
            windows_sys::Win32::Foundation::CloseHandle(process);
        }
        (name, title)
    }
}

fn window_picker(ed: &mut Editor, ui: &mut egui::Ui, pick: &Pick) -> bool {
    let t = |k: &str| ed.tr.t(&format!("settings.window-picker-dialog.{k}"));
    let wants_app = matches!(pick, Pick::AppCondition | Pick::FocusApp(..));
    ui.label(t("instructions"));
    ui.add_space(8.0);
    if let Some(deadline) = ed.countdown {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            ed.picked = Some(foreground());
            ed.countdown = None;
        } else {
            let secs = left.as_secs() + 1;
            let key = match secs {
                1 => "countdown_one",
                2..=4 => "countdown_few",
                _ => "countdown_many",
            };
            ui.label(RichText::new(ed.tr.t(&format!("settings.window-picker-dialog.{key}")).replace("{{count}}", &secs.to_string())).size(18.0));
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    } else {
        let label = if ed.picked.is_some() { t("restart-countdown") } else { t("start-countdown") };
        if ui.button(label).clicked() {
            ed.countdown = Some(Instant::now() + Duration::from_secs(5));
        }
    }
    if let Some((app, title)) = ed.picked.clone() {
        let value = if wants_app { app } else { title };
        ui.label(t("result").replace("{{name}}", &value));
        if ui.button(t("confirm")).clicked() {
            match pick {
                Pick::AppCondition | Pick::WindowCondition => {
                    let key = if wants_app { "appName" } else { "windowName" };
                    let menu = ed.menu;
                    if let Some(m) = ed.doc["menus"].get_mut(menu) {
                        if !m["conditions"].is_object() {
                            m["conditions"] = json!({});
                        }
                        m["conditions"][key] = json!(value);
                    }
                }
                Pick::FocusApp(workflow, index) | Pick::FocusWindow(workflow, index) => {
                    let key = if wants_app { "appName" } else { "windowName" };
                    if let Some(action) = selected_action(ed, workflow, *index) {
                        action[key] = json!(value);
                    }
                }
                Pick::Command(..) => {}
            }
            return true;
        }
    }
    false
}

const LANGUAGES: &[(&str, &str)] = &[
    ("ar", "Arabic"),
    ("cs", "Čeština"),
    ("da", "Dansk"),
    ("de", "Deutsch"),
    ("el", "Ελληνικά"),
    ("en", "English"),
    ("es", "Español"),
    ("fa", "Persian"),
    ("fi", "Suomi"),
    ("fr", "Français"),
    ("hi", "हिन्दी"),
    ("ia", "Interlingua"),
    ("it", "Italiano"),
    ("ja", "日本語"),
    ("kab", "Taqbaylit"),
    ("ko", "한국어"),
    ("lt", "Lietuvių"),
    ("nb-NO", "Norsk bokmål"),
    ("nl", "Nederlands"),
    ("pl", "Polski"),
    ("pt", "Português"),
    ("pt-BR", "Português (Brasil)"),
    ("ru", "Русский"),
    ("sr", "Српски"),
    ("ta", "தமிழ்"),
    ("tr", "Türkçe"),
    ("uk", "Українська"),
    ("vi", "Tiếng Việt"),
    ("zh-Hans", "简体中文"),
    ("zh-Hant", "繁體中文"),
];

const EMOJI: &str = "😀 😂 😊 😍 😎 🤔 😴 👍 👋 🙏 💪 🔥 ⭐ ✨ ❤ 💡 🎵 🎧 🎮 🎬 📷 🖼 📁 📂 📄 📝 📋 📌 ✉ 💬 🔔 🔊 🔇 ⚙ 🛠 🔒 🔑 🔍 🌐 💻 🖥 ⌨ 🖱 📱 🔋 ⏰ 📅 🗑 🚀 ✈ 🚗 🏠 🍕 ☕ 🎁 💰 📈 ✅ ❌ ⚡ 🌙 ☀ 🌈";

fn icon_theme_name(theme: &str) -> String {
    match theme {
        "material-symbols-rounded" => "Material Symbols".into(),
        "simple-icons" => "Simple Icons".into(),
        "simple-icons-colored" => "Simple Icons Colored".into(),
        "emoji" => "Emoji".into(),
        "kando" => "Orbit Actions".into(),
        "system" => "System Icons".into(),
        "base64" => "Base64".into(),
        other => other.to_string(),
    }
}

fn icon_picker(ed: &mut Editor, ui: &mut egui::Ui) -> Option<(String, String)> {
    let tone = colors(ui.ctx());
    let mut chosen = None;
    let mut themes: Vec<String> = icons::BUILTIN_THEMES.iter().map(|s| s.to_string()).collect();
    themes.extend(icons::user_themes());
    let previous = ed.icon_theme.clone();
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("icon-theme")
            .width(230.0)
            .height(400.0)
            .selected_text(icon_theme_name(&ed.icon_theme))
            .show_ui(ui, |ui| {
                for theme in &themes {
                    ui.selectable_value(&mut ed.icon_theme, theme.clone(), icon_theme_name(theme));
                }
            });
        let hint = match ed.icon_theme.as_str() {
            "emoji" => "🙂".to_string(),
            "base64" => "data:image/png;base64,…".to_string(),
            _ => ed.tr.t("settings.icon-picker-dialog.search-placeholder"),
        };
        let search = ui.add(egui::TextEdit::singleline(&mut ed.icon_search).id(egui::Id::new("icon-search")).hint_text(hint).desired_width(f32::INFINITY));
        if ui.memory(|m| m.focused().is_none()) {
            search.request_focus();
        }
    });
    if ed.icon_theme != previous {
        ed.icon_search.clear();
    }
    ui.add_space(8.0);

    let theme = ed.icon_theme.clone();
    match theme.as_str() {
        "emoji" => {
            if !ed.icon_search.trim().is_empty() && ui.button(RichText::new(format!("{}  OK", ed.icon_search.trim())).size(20.0)).clicked() {
                chosen = Some(("emoji".to_string(), ed.icon_search.trim().to_string()));
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for emoji in EMOJI.split(' ') {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(52.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(rect, 8, tone.line);
                        }
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, emoji, egui::FontId::proportional(28.0), tone.text);
                        if resp.clicked() {
                            chosen = Some(("emoji".to_string(), emoji.to_string()));
                        }
                    }
                });
            });
        }
        "base64" => {
            ui.label(RichText::new(ed.tr.t("settings.icon-picker-dialog.base64-hint")).color(tone.dim));
            if ui.button(ed.tr.t("settings.file-picker.select-file")).clicked() {
                if let Some(file) = rfd::FileDialog::new().add_filter("PNG", &["png"]).pick_file() {
                    if let Ok(bytes) = std::fs::read(file) {
                        chosen = Some(("base64".to_string(), format!("data:image/png;base64,{}", STANDARD.encode(bytes))));
                    }
                }
            }
            if !ed.icon_search.trim().is_empty() && ui.button("OK").clicked() {
                chosen = Some(("base64".to_string(), ed.icon_search.trim().to_string()));
            }
        }
        _ => {
            let filter = ed.icon_search.to_lowercase();
            let names: Vec<String> = match theme.as_str() {
                "material-symbols-rounded" => ed.icons.material.iter().filter(|(n, _)| n.contains(&filter)).map(|(n, _)| n.to_string()).collect(),
                "simple-icons" | "simple-icons-colored" => ed.icons.simple.iter().filter(|(n, _)| n.contains(&filter)).map(|(n, _)| n.to_string()).collect(),
                "kando" => icons::ACTION_ICONS.iter().filter(|(n, _)| n.contains(&filter)).map(|(n, _)| n.to_string()).collect(),
                "system" => ed.icons.apps.iter().filter(|a| a.name.to_lowercase().contains(&filter)).map(|a| a.name.clone()).collect(),
                user => icons::user_icons(user).into_iter().filter(|n| n.to_lowercase().contains(&filter)).collect(),
            };
            let cell = 64.0;
            let per_row = ((ui.available_width() / cell).floor() as usize).max(1);
            let rows = names.len().div_ceil(per_row);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, cell, rows, |ui, range| {
                for r in range {
                    ui.horizontal(|ui| {
                        for name in names.iter().skip(r * per_row).take(per_row) {
                            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(cell - 6.0), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(rect, 8, tone.line);
                            }
                            ed.icons.draw(ui.ctx(), ui.painter(), rect.shrink(8.0), &theme, name, tone.text);
                            if resp.on_hover_text(name).clicked() {
                                chosen = Some((theme.clone(), name.clone()));
                            }
                        }
                    });
                }
            });
        }
    }
    chosen
}

fn action_picker(ed: &mut Editor, ui: &mut egui::Ui) -> Option<&'static str> {
    let tone = colors(ui.ctx());
    let mut picked = None;
    let search = ui.add(egui::TextEdit::singleline(&mut ed.action_search).id(egui::Id::new("action-search")).hint_text(ed.tr.t("settings.workflow-editor.action-picker.search-placeholder")).desired_width(f32::INFINITY));
    if ui.memory(|m| m.focused().is_none()) {
        search.request_focus();
    }
    ui.add_space(6.0);
    let filter = ed.action_search.to_lowercase();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for &kind in ACTIONS {
            let name = ed.tr.t(&format!("menu-actions.{kind}.name"));
            let desc = ed.tr.t(&format!("menu-actions.{kind}.description"));
            if !filter.is_empty() && !name.to_lowercase().contains(&filter) && !desc.to_lowercase().contains(&filter) {
                continue;
            }
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 58.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(rect, 8, tone.line);
            }
            let icon_rect = egui::Rect::from_center_size(egui::pos2(rect.left() + 28.0, rect.center().y), Vec2::splat(30.0));
            ed.icons.draw(ui.ctx(), ui.painter(), icon_rect, "kando", action_meta(kind).icon, tone.text);
            ui.painter().text(egui::pos2(rect.left() + 56.0, rect.top() + 18.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(17.0), tone.text);
            let mut job = egui::text::LayoutJob::single_section(desc.clone(), egui::TextFormat::simple(egui::FontId::proportional(13.0), tone.dim));
            job.wrap = egui::text::TextWrapping { max_width: rect.width() - 70.0, max_rows: 1, break_anywhere: false, overflow_character: Some('…') };
            let line = ui.fonts(|f| f.layout_job(job));
            ui.painter().galley(egui::pos2(rect.left() + 56.0, rect.top() + 32.0), line, tone.dim);
            if resp.on_hover_text(desc).clicked() {
                picked = Some(kind);
            }
        }
    });
    picked
}
