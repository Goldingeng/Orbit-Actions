use crate::style::colors;
use eframe::egui::{self, pos2, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use serde_json::{json, Value};

use crate::math;
use crate::{icons, item_type, Editor};

#[derive(Clone, PartialEq)]
pub enum Source {
    Child(usize),
    Template(String),
}

#[derive(Clone)]
pub struct Drag {
    pub source: Source,
}

#[derive(Default)]
pub struct Motion {
    stack: Vec<usize>,
    angles: std::collections::HashMap<String, f32>,
    hover: Option<(usize, f64)>,
}

fn item_keys(children: &[Value]) -> Vec<String> {
    let mut seen = std::collections::HashMap::new();
    children
        .iter()
        .map(|c| {
            let base = format!("{}|{}|{}", c["name"], c["icon"], c["type"]);
            let n = seen.entry(base.clone()).or_insert(0);
            *n += 1;
            format!("{base}|{n}")
        })
        .collect()
}

fn angle_gap(a: f32, b: f32) -> f32 {
    ((a - b + 540.0).rem_euclid(360.0) - 180.0).abs()
}

fn insert_slot(rest_fixed: &[Option<f32>], parent: Option<f32>, at: f32) -> (usize, Vec<f32>) {
    let mut best = (0, f32::MAX, vec![]);
    for slot in 0..=rest_fixed.len() {
        let mut fixed = rest_fixed.to_vec();
        fixed.insert(slot, None);
        let angles = math::item_angles(&fixed, parent);
        let gap = angle_gap(angles[slot], at);
        if gap < best.1 {
            best = (slot, gap, angles);
        }
    }
    (best.0, best.2)
}

pub const TEMPLATES: &[&str] = &[
    "submenu",
    "execute-command",
    "execute-macro",
    "open-file",
    "open-menu",
    "open-settings",
    "open-uri",
    "set-clipboard",
    "simulate-hotkey",
];

const IDLE: Color32 = Color32::from_rgba_premultiplied(77, 77, 77, 77);
const CHOSEN: Color32 = Color32::from_rgba_premultiplied(105, 105, 105, 150);

fn to_pos(c: Pos2, angle: f32, distance: f32) -> Pos2 {
    let d = math::dir(angle, distance);
    pos2(c.x + d.x, c.y + d.y)
}

fn pointer_angle(c: Pos2, p: Pos2) -> f32 {
    math::angle_of(math::v(p.x - c.x, p.y - c.y))
}

pub fn fixed_angles(children: &[Value]) -> Vec<Option<f32>> {
    children.iter().map(|c| c["angle"].as_f64().map(|a| a as f32)).collect()
}

impl Editor {
    pub fn parent_angle(&self) -> Option<f32> {
        let mut parent: Option<f32> = None;
        let mut path = vec![];
        for &i in &self.stack {
            let item = self.node(&path)?;
            let children = item["children"].as_array()?;
            let angles = math::item_angles(&fixed_angles(children), parent);
            parent = Some((angles.get(i).copied().unwrap_or(0.0) + 180.0) % 360.0);
            path.push(i);
        }
        parent
    }

    pub fn template_item(&self, kind: &str) -> Value {
        if kind == "submenu" {
            return json!({
                "type": "submenu",
                "name": self.t("menu-items.submenu.name"),
                "icon": "submenu-item.svg",
                "iconTheme": "kando",
                "children": [],
                "activateWorkflow": { "quickSelectKey": "Backspace", "actions": [{ "type": "close-submenu" }] }
            });
        }
        let meta = crate::props::action_meta(kind);
        let mut actions = vec![];
        if meta.inhibit {
            actions.push(json!({ "type": "inhibit-shortcuts" }));
        }
        if meta.delayed {
            actions.push(json!({ "type": "close-menu" }));
        }
        actions.push(meta.create());
        if !meta.delayed && kind != "open-menu" {
            actions.push(json!({ "type": "close-menu" }));
        }
        json!({
            "type": "button",
            "name": self.t(&format!("menu-actions.{kind}.name")),
            "icon": meta.icon,
            "iconTheme": "kando",
            "selectWorkflow": { "actions": actions }
        })
    }

    pub fn drop_file(&mut self, path: &std::path::Path) {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let lower = name.to_lowercase();
        let full = path.to_string_lossy().to_string();
        let stem = match name.rfind('.') {
            Some(i) => name[..i].to_string(),
            None => name.clone(),
        };
        let item = if lower.ends_with(".lnk") || lower.ends_with(".exe") || lower.ends_with(".bat") || lower.ends_with(".cmd") {
            let app = self.icons.apps.iter().find(|a| a.name == stem).map(|a| a.name.clone());
            json!({
                "type": "button",
                "name": stem,
                "icon": app.clone().unwrap_or_else(|| "command-item.svg".into()),
                "iconTheme": if app.is_some() { "system" } else { "kando" },
                "selectWorkflow": { "actions": [
                    { "type": "execute-command", "command": format!("\"{full}\""), "detached": true, "isolated": true },
                    { "type": "close-menu" }
                ] }
            })
        } else {
            let ext = lower.rsplit('.').next().unwrap_or("");
            let icon = if path.is_dir() {
                "folder"
            } else {
                match ext {
                    "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tif" | "tiff" => "image",
                    "mp4" | "mkv" | "avi" | "mov" | "webm" | "wmv" => "video_file",
                    "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" => "audio_file",
                    "txt" | "md" | "csv" | "log" | "json" | "xml" | "html" | "css" | "js" => "text_snippet",
                    _ => "draft",
                }
            };
            json!({
                "type": "button",
                "name": name,
                "icon": icon,
                "iconTheme": "material-symbols-rounded",
                "selectWorkflow": { "actions": [{ "type": "open-file", "path": full }, { "type": "close-menu" }] }
            })
        };
        let stack = self.stack.clone();
        if let Some(list) = self.children_mut(&stack) {
            list.push(item);
            self.selected = Some(list.len() - 1);
        }
    }

    fn children_mut(&mut self, path: &[usize]) -> Option<&mut Vec<Value>> {
        let item = self.node_mut(path)?;
        if !item["children"].is_array() {
            item["children"] = json!([]);
        }
        item["children"].as_array_mut()
    }

    pub fn preview(&mut self, ui: &mut egui::Ui) {
    let tone = colors(ui.ctx());
        let full = ui.available_rect_before_wrap();
        let footer = Rect::from_min_max(pos2(full.left(), full.bottom() - 210.0), full.max);
        let stage = Rect::from_min_max(full.min, pos2(full.right(), footer.top()));
        ui.allocate_rect(full, Sense::hover());

        let Some(center_item) = self.node(&self.stack.clone()).cloned() else {
            self.footer(ui, footer);
            return;
        };
        let m = stage.width().min(stage.height()) * 0.98;
        let c = stage.center();
        let painter = ui.painter_at(stage);
        let children: Vec<Value> = center_item["children"].as_array().cloned().unwrap_or_default();
        let parent = self.parent_angle();
        let angles = math::item_angles(&fixed_angles(&children), parent);

        let center_r = m * 0.125;
        let child_r = m * 0.06;
        let child_d = m * 0.30;

        let pointer = ui.ctx().pointer_hover_pos();
        let released = ui.input(|i| i.pointer.any_released());
        let dragging_child = match &self.drag {
            Some(Drag { source: Source::Child(i) }) => Some(*i),
            _ => None,
        };

        if let Some(p) = parent {
            let back = to_pos(c, p, child_d);
            let rect = Rect::from_center_size(back, Vec2::splat(m * 0.08));
            let resp = ui.interact(rect, ui.id().with("backlink"), Sense::click());
            let hot = resp.hovered() || (self.drag.is_some() && pointer.is_some_and(|q| rect.contains(q)));
            painter.circle_filled(back, m * 0.04, if hot { tone.line } else { IDLE });
            painter.text(back, Align2::CENTER_CENTER, self.icons.glyph("arrow_back"), icons::material_font(m * 0.04), tone.text);
            if resp.clicked() {
                let last = self.stack.pop();
                self.selected = last;
            }
            if released && hot {
                if let Some(drag) = self.drag.take() {
                    self.drop_to_parent(drag);
                }
            }
        }

        let center_rect = Rect::from_center_size(c, Vec2::splat(center_r * 2.0));
        let center_resp = ui.interact(center_rect, ui.id().with("center"), Sense::click());
        let center_selected = self.selected.is_none();
        painter.circle_filled(c, center_r, if center_selected { CHOSEN } else { IDLE });
        if center_selected {
            painter.circle_stroke(c, center_r, Stroke::new(2.0, tone.text));
        }
        let theme = center_item["iconTheme"].as_str().unwrap_or("").to_string();
        let icon = center_item["icon"].as_str().unwrap_or("").to_string();
        self.icons.draw(ui.ctx(), &painter, center_rect.shrink(center_r * 0.25), &theme, &icon, tone.text);
        if center_resp.clicked() {
            self.selected = None;
        }

        let drag_angle = pointer.filter(|_| self.drag.is_some()).map(|p| pointer_angle(c, p));
        let order: Vec<usize> = (0..children.len()).collect();
        let mut shown_angles = angles.clone();
        let mut drop_into = None;
        let mut swap_with = None;
        let mut charge = 0.0;
        let mut hovered_menu = None;
        if let (Some(at), Some(p)) = (drag_angle, pointer) {
            let locked = dragging_child.is_some_and(|i| children[i]["angle"].is_number());
            let over = |i: usize| item_type(&children[i]) == "submenu" && to_pos(c, angles[i], child_d).distance(p) < child_r;
            match dragging_child {
                Some(dragged) if stage.contains(p) && p.distance(c) > center_r => {
                    let target = (0..children.len())
                        .filter(|&i| i == dragged || (!locked && !children[i]["angle"].is_number()) || over(i))
                        .min_by(|&a, &b| angle_gap(angles[a], at).total_cmp(&angle_gap(angles[b], at)))
                        .filter(|&t| t != dragged);
                    if let Some(t) = target {
                        if over(t) {
                            hovered_menu = Some(t);
                            let now = ui.input(|i| i.time);
                            let since = match self.motion.hover {
                                Some((i, since)) if i == t => since,
                                _ => now,
                            };
                            self.motion.hover = Some((t, since));
                            charge = ((now - since) / 1.2).min(1.0) as f32;
                            if charge >= 1.0 {
                                drop_into = Some(t);
                            } else {
                                ui.ctx().request_repaint();
                            }
                        }
                        if drop_into.is_none() && !locked {
                            swap_with = Some(t);
                        }
                    }
                }
                None if stage.contains(p) => {
                    drop_into = (0..children.len()).find(|&i| item_type(&children[i]) == "submenu" && to_pos(c, angles[i], child_d).distance(p) < child_r * 0.8);
                    if drop_into.is_none() {
                        let (slot, preview_angles) = insert_slot(&fixed_angles(&children), parent, at);
                        for i in order.iter().copied() {
                            shown_angles[i] = preview_angles[if i < slot { i } else { i + 1 }];
                        }
                    }
                }
                _ => {}
            }
        }
        if hovered_menu.is_none() {
            self.motion.hover = None;
        }

        if self.motion.stack != self.stack {
            self.motion.stack = self.stack.clone();
            self.motion.angles.clear();
        }
        let keys = item_keys(&children);
        let dt = ui.input(|i| i.stable_dt).min(0.05);
        let step = 1.0 - (-dt * 16.0).exp();
        let mut moving = false;
        let targets = shown_angles.clone();
        for (i, key) in keys.iter().enumerate() {
            let target = targets[i];
            let current = self.motion.angles.entry(key.clone()).or_insert(target);
            let diff = (target - *current + 540.0).rem_euclid(360.0) - 180.0;
            if diff.abs() < 0.2 {
                *current = target;
            } else {
                *current = (*current + diff * step).rem_euclid(360.0);
                moving = true;
            }
            shown_angles[i] = *current;
        }
        self.motion.angles.retain(|k, _| keys.contains(k));
        if moving {
            ui.ctx().request_repaint();
        }
        let mut lifted = None;

        let mut clicked_child = None;
        let mut entered = None;
        for &i in &order {
            let child = &children[i];
            let angle = shown_angles[i];
            let at = to_pos(c, angle, child_d);
            let rect = Rect::from_center_size(at, Vec2::splat(child_r * 2.0));
            let resp = ui.interact(rect, ui.id().with(("child", i)), Sense::click_and_drag());
            let selected = self.selected == Some(i);
            let is_dragged = dragging_child == Some(i);

            if item_type(child) == "submenu" && !is_dragged {
                let grand: Vec<Value> = child["children"].as_array().cloned().unwrap_or_default();
                let grand_angles = math::item_angles(&fixed_angles(&grand), Some((angle + 180.0) % 360.0));
                for a in grand_angles {
                    painter.circle_filled(to_pos(at, a, m * 0.072), m * 0.02, IDLE);
                }
            }

            let into = drop_into == Some(i);
            let hot = into || swap_with == Some(i);
            if is_dragged {
                lifted = Some(i);
            } else {
                let fill = if selected || hot { CHOSEN } else if resp.hovered() { tone.line } else { IDLE };
                painter.circle_filled(at, child_r, fill);
                if selected || hot {
                    painter.circle_stroke(at, child_r, Stroke::new(if into { 4.0 } else { 2.0 }, tone.text));
                }
                if hovered_menu == Some(i) && charge > 0.0 && charge < 1.0 {
                    let ring: Vec<Pos2> = (0..=48)
                        .map(|n| {
                            let a = std::f32::consts::TAU * charge * n as f32 / 48.0 - std::f32::consts::FRAC_PI_2;
                            pos2(at.x + a.cos() * (child_r + 6.0), at.y + a.sin() * (child_r + 6.0))
                        })
                        .collect();
                    painter.add(egui::Shape::line(ring, Stroke::new(3.0, tone.text)));
                }
                let theme = child["iconTheme"].as_str().unwrap_or("").to_string();
                let icon = child["icon"].as_str().unwrap_or("").to_string();
                self.icons.draw(ui.ctx(), &painter, Rect::from_center_size(at, Vec2::splat(child_r * 1.6)), &theme, &icon, tone.text);
            }

            if resp.clicked() {
                clicked_child = Some(i);
            }
            if resp.double_clicked() && item_type(child) == "submenu" {
                entered = Some(i);
            }
            if resp.drag_started() {
                self.drag = Some(Drag { source: Source::Child(i) });
                self.selected = Some(i);
            }
            if is_dragged && child["angle"].is_number() {
                if let Some(a) = drag_angle {
                    let snapped = (a / 15.0).round() * 15.0 % 360.0;
                    let path: Vec<usize> = self.stack.iter().copied().chain(Some(i)).collect();
                    if let Some(item) = self.node_mut(&path) {
                        item["angle"] = json!(snapped);
                    }
                }
            }

            if (selected || child["angle"].is_number()) && dragging_child.is_none() {
                let lock_at = to_pos(c, angle, m * 0.175);
                let lock_rect = Rect::from_center_size(lock_at, Vec2::splat(m * 0.04));
                let lock_resp = ui.interact(lock_rect, ui.id().with(("lock", i)), Sense::click()).on_hover_text(self.t("settings.fix-menu-item-angle"));
                let locked = child["angle"].is_number();
                painter.circle_filled(lock_at, m * 0.02, if lock_resp.hovered() { tone.line } else { IDLE });
                let glyph = self.icons.glyph(if locked { "lock" } else { "lock_open" });
                painter.text(lock_at, Align2::CENTER_CENTER, glyph, icons::material_font(m * 0.024), tone.text);
                if lock_resp.clicked() {
                    let path: Vec<usize> = self.stack.iter().copied().chain(Some(i)).collect();
                    if let Some(item) = self.node_mut(&path) {
                        if locked {
                            item.as_object_mut().map(|o| o.remove("angle"));
                        } else {
                            item["angle"] = json!(targets[i].round());
                        }
                    }
                }
            }

            if selected && dragging_child.is_none() {
                let mut tools = vec![(-38.0, "delete", "settings.delete-menu-item"), (38.0, "content_copy", "settings.duplicate-menu-item")];
                if item_type(child) == "submenu" {
                    tools.push((0.0, "edit", "settings.edit-submenu"));
                }
                for (offset, glyph, tip) in tools {
                    let tool_at = to_pos(at, angle + offset, m * 0.105);
                    let tool_rect = Rect::from_center_size(tool_at, Vec2::splat(m * 0.05));
                    let tool_resp = ui.interact(tool_rect, ui.id().with(("tool", i, glyph)), Sense::click()).on_hover_text(self.t(tip));
                    painter.circle_filled(tool_at, m * 0.025, if tool_resp.hovered() { tone.line } else { IDLE });
                    painter.text(tool_at, Align2::CENTER_CENTER, self.icons.glyph(glyph), icons::material_font(m * 0.028), tone.text);
                    if tool_resp.clicked() {
                        match glyph {
                            "delete" => {
                                let stack = self.stack.clone();
                                if let Some(list) = self.children_mut(&stack) {
                                    list.remove(i);
                                }
                                self.selected = None;
                            }
                            "content_copy" => {
                                let stack = self.stack.clone();
                                let mut copy = child.clone();
                                copy.as_object_mut().map(|o| o.remove("angle"));
                                if let Some(list) = self.children_mut(&stack) {
                                    list.insert(i + 1, copy);
                                }
                                self.selected = Some(i + 1);
                            }
                            _ => {
                                self.stack.push(i);
                                self.selected = None;
                            }
                        }
                        return;
                    }
                }
            }
        }

        if let Some(i) = clicked_child {
            self.selected = Some(i);
        }
        if let Some(i) = entered {
            self.stack.push(i);
            self.selected = None;
        }

        if let (Some(i), Some(p)) = (lifted, pointer) {
            let child = &children[i];
            if item_type(child) == "submenu" {
                let grand: Vec<Value> = child["children"].as_array().cloned().unwrap_or_default();
                let away = (pointer_angle(c, p) + 180.0) % 360.0;
                for a in math::item_angles(&fixed_angles(&grand), Some(away)) {
                    painter.circle_filled(to_pos(p, a, m * 0.072), m * 0.02, IDLE);
                }
            }
            painter.circle_filled(p, child_r, CHOSEN);
            painter.circle_stroke(p, child_r, Stroke::new(2.0, tone.text));
            let theme = child["iconTheme"].as_str().unwrap_or("").to_string();
            let icon = child["icon"].as_str().unwrap_or("").to_string();
            self.icons.draw(ui.ctx(), &painter, Rect::from_center_size(p, Vec2::splat(child_r * 1.6)), &theme, &icon, tone.text);
        }

        if let (Some(Drag { source: Source::Template(kind) }), Some(p)) = (&self.drag, pointer) {
            painter.circle_filled(p, child_r, CHOSEN);
            let meta_icon = if kind == "submenu" { "submenu-item.svg" } else { crate::props::action_meta(kind).icon };
            self.icons.draw(ui.ctx(), &painter, Rect::from_center_size(p, Vec2::splat(child_r * 1.6)), "kando", meta_icon, tone.text);
        }

        if released && self.drag.is_some() {
            let drag = self.drag.take().unwrap();
            if let Some(p) = pointer {
                if footer.contains(p) {
                    if let Source::Child(i) = drag.source {
                        let stack = self.stack.clone();
                        if let Some(list) = self.children_mut(&stack) {
                            list.remove(i);
                        }
                        self.selected = None;
                    }
                } else if stage.contains(p) {
                    match (&drag.source, swap_with) {
                        (Source::Child(dragged), Some(target)) if drop_into.is_none() => {
                            let (dragged, stack) = (*dragged, self.stack.clone());
                            if let Some(list) = self.children_mut(&stack) {
                                list.swap(dragged, target);
                            }
                            self.selected = Some(target);
                        }
                        (Source::Child(_), None) if drop_into.is_none() => {}
                        _ => {
                            let at = pointer_angle(c, p);
                            self.drop_at(drag, at, drop_into, &children, parent);
                        }
                    }
                }
            }
        }

        self.footer(ui, footer);
    }

    fn take_item(&mut self, source: &Source) -> Option<Value> {
        match source {
            Source::Template(kind) => Some(self.template_item(kind)),
            Source::Child(i) => {
                let stack = self.stack.clone();
                let list = self.children_mut(&stack)?;
                (*i < list.len()).then(|| list.remove(*i))
            }
        }
    }

    fn drop_at(&mut self, drag: Drag, at: f32, into: Option<usize>, children: &[Value], parent: Option<f32>) {
        if let Source::Child(i) = drag.source {
            if children[i]["angle"].is_number() && into.is_none() {
                return;
            }
        }
        let original = match drag.source {
            Source::Child(i) => Some(i),
            _ => None,
        };
        let template = matches!(drag.source, Source::Template(_));
        let Some(item) = self.take_item(&drag.source) else { return };
        let stack = self.stack.clone();

        if let Some(mut target) = into {
            if original.is_some_and(|o| o < target) {
                target -= 1;
            }
            let mut path = stack.clone();
            path.push(target);
            let mut item = item;
            item.as_object_mut().map(|o| o.remove("angle"));
            if let Some(list) = self.children_mut(&path) {
                list.push(item);
            }
            self.selected = Some(target);
            if template {
            }
            return;
        }

        let rest: Vec<Value> = self.node(&stack).and_then(|n| n["children"].as_array().cloned()).unwrap_or_default();
        let (index, _) = insert_slot(&fixed_angles(&rest), parent, at);
        if let Some(list) = self.children_mut(&stack) {
            list.insert(index, item);
        }
        self.selected = Some(index);
        if template {
        }
    }

    fn drop_to_parent(&mut self, drag: Drag) {
        if self.stack.is_empty() {
            return;
        }
        let Some(mut item) = self.take_item(&drag.source) else { return };
        item.as_object_mut().map(|o| o.remove("angle"));
        let mut up = self.stack.clone();
        let here = up.pop().unwrap();
        if let Some(list) = self.children_mut(&up) {
            list.insert(here + 1, item);
        }
        self.selected = None;
    }

    fn footer(&mut self, ui: &mut egui::Ui, rect: Rect) {
    let tone = colors(ui.ctx());
        let painter = ui.painter_at(rect);
        let tab = Rect::from_center_size(pos2(rect.center().x, rect.top() + 22.0), Vec2::new(266.0, 42.0));
        let band = Rect::from_min_max(pos2(rect.left(), tab.bottom()), rect.max);
        painter.rect_filled(band, CornerRadius::ZERO, tone.stage);
        painter.rect_filled(tab, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, tone.stage);
        painter.rect_stroke(tab, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, Stroke::new(1.0, tone.line), egui::StrokeKind::Inside);

        if matches!(self.drag, Some(Drag { source: Source::Child(_) })) {
            painter.text(tab.center(), Align2::CENTER_CENTER, self.t("settings.drop-here-to-delete"), FontId::proportional(17.0), tone.text);
            painter.text(band.center(), Align2::CENTER_CENTER, self.icons.glyph("delete"), icons::material_font(64.0), tone.dim);
            return;
        }
        painter.text(tab.center(), Align2::CENTER_CENTER, self.t("settings.add-menu-items"), FontId::proportional(17.0), tone.text);

        let per_row = 7usize;
        let size = ((band.width() - 40.0) / (per_row as f32 * 1.3 - 0.3)).min(78.0);
        let gap = size * 0.3;
        let rows: Vec<&[&str]> = TEMPLATES.chunks(per_row).collect();
        for (r, row) in rows.iter().enumerate() {
            let width = row.len() as f32 * size + (row.len() as f32 - 1.0) * gap;
            let mut x = band.center().x - width / 2.0 + size / 2.0;
            let y = band.top() + 16.0 + size / 2.0 + r as f32 * (size + 12.0);
            for &kind in row.iter() {
                let at = pos2(x, y);
                let rect = Rect::from_center_size(at, Vec2::splat(size));
                let name = if kind == "submenu" { self.t("menu-items.submenu.name") } else { self.t(&format!("menu-actions.{kind}.name")) };
                let desc = if kind == "submenu" { self.t("menu-items.submenu.description") } else { self.t(&format!("menu-actions.{kind}.description")) };
                let resp = ui
                    .interact(rect, ui.id().with(("template", kind)), Sense::click_and_drag())
                    .on_hover_text(format!("{name}\n{desc}\n\n{}", self.t("settings.add-menu-item-hint")));
                painter.circle_filled(at, size / 2.0, if resp.hovered() { tone.hover } else { tone.button });
                let icon = if kind == "submenu" { "submenu-item.svg" } else { crate::props::action_meta(kind).icon };
                self.icons.draw(ui.ctx(), &painter, rect.shrink(size * 0.2), "kando", icon, tone.text);
                if resp.drag_started() {
                    self.drag = Some(Drag { source: Source::Template(kind.to_string()) });
                }
                if resp.clicked() {
                    let item = self.template_item(kind);
                    let stack = self.stack.clone();
                    if let Some(list) = self.children_mut(&stack) {
                        list.push(item);
                        self.selected = Some(list.len() - 1);
                    }
                }
                x += size + gap;
            }
        }
    }
}
