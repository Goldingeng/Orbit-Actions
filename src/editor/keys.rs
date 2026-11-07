use eframe::egui::{self, Event, Key};
use serde_json::{json, Value};

use crate::{Editor, Recording};

pub fn dom_code(key: Key) -> Option<String> {
    let name = key.name();
    if name.len() == 1 && name.chars().all(|c| c.is_ascii_alphabetic()) {
        return Some(format!("Key{name}"));
    }
    if name.len() == 1 && name.chars().all(|c| c.is_ascii_digit()) {
        return Some(format!("Digit{name}"));
    }
    if name.starts_with('F') && name[1..].parse::<u32>().is_ok() {
        return Some(name.to_string());
    }
    let code = match key {
        Key::ArrowDown => "ArrowDown",
        Key::ArrowLeft => "ArrowLeft",
        Key::ArrowRight => "ArrowRight",
        Key::ArrowUp => "ArrowUp",
        Key::Escape => "Escape",
        Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::Enter => "Enter",
        Key::Space => "Space",
        Key::Insert => "Insert",
        Key::Delete => "Delete",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::Minus => "Minus",
        Key::Equals | Key::Plus => "Equal",
        Key::OpenBracket => "BracketLeft",
        Key::CloseBracket => "BracketRight",
        Key::Backslash => "Backslash",
        Key::Semicolon => "Semicolon",
        Key::Quote => "Quote",
        Key::Comma => "Comma",
        Key::Period => "Period",
        Key::Slash => "Slash",
        Key::Backtick => "Backquote",
        _ => return None,
    };
    Some(code.to_string())
}

fn accelerator(key: Key) -> Option<String> {
    let name = key.name();
    if name.len() == 1 && name.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Some(name.to_uppercase());
    }
    if name.starts_with('F') && name[1..].parse::<u32>().is_ok() {
        return Some(name.to_string());
    }
    let s = match key {
        Key::ArrowDown => "Down",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        Key::ArrowUp => "Up",
        Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::Enter => "Enter",
        Key::Space => "Space",
        Key::Insert => "Insert",
        Key::Delete => "Delete",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::Minus => "-",
        Key::Equals | Key::Plus => "=",
        Key::Comma => ",",
        Key::Period => ".",
        Key::Slash => "/",
        Key::Backtick => "`",
        _ => return None,
    };
    Some(s.to_string())
}

fn quick_key(key: Key) -> Option<String> {
    let name = key.name();
    if name.len() == 1 {
        return Some(name.to_uppercase());
    }
    let s = match key {
        Key::Backspace => "Backspace",
        Key::Enter => "Enter",
        Key::Space => "Space",
        Key::Tab => "Tab",
        Key::Delete => "Delete",
        _ => return accelerator(key),
    };
    Some(s.to_string())
}

fn modifier_codes(m: egui::Modifiers) -> Vec<&'static str> {
    let mut out = vec![];
    if m.ctrl {
        out.push("ControlLeft");
    }
    if m.shift {
        out.push("ShiftLeft");
    }
    if m.alt {
        out.push("AltLeft");
    }
    out
}

fn action_mut<'a>(ed: &'a mut Editor, workflow: &str, index: usize) -> Option<&'a mut Value> {
    let path = ed.selected_path();
    ed.node_mut(&path)?.get_mut(workflow)?.get_mut("actions")?.get_mut(index)
}

pub fn record(ed: &mut Editor, ctx: &egui::Context) {
    if ed.recording == Recording::None {
        return;
    }
    let events = ctx.input(|i| i.events.clone());
    let mods = ctx.input(|i| i.modifiers);

    if let Recording::Macro(workflow, index) = ed.recording.clone() {
        let before = ed.macro_mods;
        let mut changes = vec![];
        for (now, was, code) in [(mods.ctrl, before.ctrl, "ControlLeft"), (mods.shift, before.shift, "ShiftLeft"), (mods.alt, before.alt, "AltLeft")] {
            if now != was {
                changes.push(json!({ "type": if now { "keyDown" } else { "keyUp" }, "key": code, "delay": 10 }));
            }
        }
        ed.macro_mods = mods;
        if let Some(action) = action_mut(ed, &workflow, index) {
            if !action["macro"].is_array() {
                action["macro"] = json!([]);
            }
            for c in changes {
                action["macro"].as_array_mut().unwrap().push(c);
            }
        }
    }

    for event in events {
        let Event::Key { key, physical_key, pressed, repeat, modifiers } = event else { continue };
        if repeat {
            continue;
        }
        let key = physical_key.unwrap_or(key);
        if pressed && key == Key::Escape && !matches!(ed.recording, Recording::Hotkey(..)) {
            ed.recording = Recording::None;
            return;
        }

        match ed.recording.clone() {
            Recording::Shortcut if pressed => {
                let Some(k) = accelerator(key) else { continue };
                let mut parts = vec![];
                if modifiers.ctrl {
                    parts.push("Control".to_string());
                }
                if modifiers.shift {
                    parts.push("Shift".to_string());
                }
                if modifiers.alt {
                    parts.push("Alt".to_string());
                }
                parts.push(k);
                let menu = ed.menu;
                if let Some(m) = ed.doc["menus"].get_mut(menu) {
                    m["shortcut"] = json!(parts.join("+"));
                }
                ed.recording = Recording::None;
            }
            Recording::QuickKey(workflow) if pressed => {
                let Some(k) = quick_key(key) else { continue };
                let path = ed.selected_path();
                if let Some(item) = ed.node_mut(&path) {
                    if !item[workflow.as_str()].is_object() {
                        item[workflow.as_str()] = json!({ "actions": [] });
                    }
                    item[workflow.as_str()]["quickSelectKey"] = json!(k);
                }
                ed.recording = Recording::None;
            }
            Recording::Hotkey(workflow, index) if pressed => {
                let Some(code) = dom_code(key) else { continue };
                let mut parts: Vec<String> = modifier_codes(modifiers).into_iter().map(String::from).collect();
                parts.push(code);
                if let Some(action) = action_mut(ed, &workflow, index) {
                    action["hotkey"] = json!(parts.join("+"));
                }
                ed.recording = Recording::None;
            }
            Recording::Macro(workflow, index) => {
                let Some(code) = dom_code(key) else { continue };
                if let Some(action) = action_mut(ed, &workflow, index) {
                    if let Some(list) = action["macro"].as_array_mut() {
                        list.push(json!({ "type": if pressed { "keyDown" } else { "keyUp" }, "key": code, "delay": 10 }));
                    }
                }
            }
            _ => {}
        }
    }
}
