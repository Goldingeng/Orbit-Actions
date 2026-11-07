use base64::{engine::general_purpose::STANDARD, Engine};
use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, Painter, Rect, TextureHandle};
use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc};

use crate::apps;

pub const BUILTIN_THEMES: &[&str] = &["material-symbols-rounded", "simple-icons", "simple-icons-colored", "emoji", "kando", "system", "base64"];

pub const ACTION_ICONS: &[(&str, &str)] = &[
    ("clipboard-item.svg", include_str!("../../assets/action-icons/clipboard-item.svg")),
    ("close-menu-item.svg", include_str!("../../assets/action-icons/close-menu-item.svg")),
    ("close-submenu-item.svg", include_str!("../../assets/action-icons/close-submenu-item.svg")),
    ("command-item.svg", include_str!("../../assets/action-icons/command-item.svg")),
    ("delay-item.svg", include_str!("../../assets/action-icons/delay-item.svg")),
    ("edit-submenu.svg", include_str!("../../assets/action-icons/edit-submenu.svg")),
    ("file-item.svg", include_str!("../../assets/action-icons/file-item.svg")),
    ("focus-window-item.svg", include_str!("../../assets/action-icons/focus-window-item.svg")),
    ("hotkey-item.svg", include_str!("../../assets/action-icons/hotkey-item.svg")),
    ("inhibit-shortcuts-item.svg", include_str!("../../assets/action-icons/inhibit-shortcuts-item.svg")),
    ("macro-item.svg", include_str!("../../assets/action-icons/macro-item.svg")),
    ("redirect-item.svg", include_str!("../../assets/action-icons/redirect-item.svg")),
    ("settings-item.svg", include_str!("../../assets/action-icons/settings-item.svg")),
    ("submenu-item.svg", include_str!("../../assets/action-icons/submenu-item.svg")),
    ("text-item.svg", include_str!("../../assets/action-icons/text-item.svg")),
    ("uri-item.svg", include_str!("../../assets/action-icons/uri-item.svg")),
    ("workflow-activate.svg", include_str!("../../assets/action-icons/workflow-activate.svg")),
    ("workflow-hover.svg", include_str!("../../assets/action-icons/workflow-hover.svg")),
    ("workflow-select.svg", include_str!("../../assets/action-icons/workflow-select.svg")),
];

pub struct Icons {
    pub material: Vec<(&'static str, char)>,
    pub simple: Vec<(&'static str, char)>,
    material_map: HashMap<&'static str, char>,
    simple_map: HashMap<&'static str, (char, Option<Color32>)>,
    textures: HashMap<String, Option<TextureHandle>>,
    pub apps: Vec<apps::App>,
    apps_rx: Option<mpsc::Receiver<Vec<apps::App>>>,
    app_map: HashMap<String, usize>,
    icon_tx: mpsc::SyncSender<String>,
    icon_rx: mpsc::Receiver<(String, Option<egui::ColorImage>)>,
    pending: HashSet<String>,
}

fn hex(s: &str) -> Option<Color32> {
    let h = s.strip_prefix('#')?;
    let n = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    Some(Color32::from_rgb(n(0)?, n(2)?, n(4)?))
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    if let Ok(bytes) = std::fs::read(format!("{windir}\\Fonts\\segoeui.ttf")) {
        fonts.font_data.insert("segoe".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "segoe".into());
    }
    if let Ok(bytes) = std::fs::read(format!("{windir}\\Fonts\\seguisb.ttf")) {
        fonts.font_data.insert("segoe-bold".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.insert(FontFamily::Name("bold".into()), vec!["segoe-bold".into(), "segoe".into()]);
    } else {
        fonts.families.insert(FontFamily::Name("bold".into()), vec!["segoe".into()]);
    }
    fonts.font_data.insert("material".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/material.ttf"))));
    fonts.font_data.insert("simple".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/simple.ttf"))));
    fonts.families.insert(FontFamily::Name("material".into()), vec!["material".into()]);
    fonts.families.insert(FontFamily::Name("simple".into()), vec!["simple".into()]);
    ctx.set_fonts(fonts);
}

pub fn material_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("material".into()))
}

pub fn user_themes() -> Vec<String> {
    let mut out = vec![];
    if let Ok(entries) = std::fs::read_dir(crate::data_dir().join("icon-themes")) {
        for e in entries.flatten() {
            if e.path().is_dir() {
                out.push(e.file_name().to_string_lossy().to_string());
            }
        }
    }
    out.sort();
    out
}

pub fn user_icons(theme: &str) -> Vec<String> {
    let mut out = vec![];
    if let Ok(entries) = std::fs::read_dir(crate::data_dir().join("icon-themes").join(theme)) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if lower.ends_with(".png") || lower.ends_with(".svg") {
                out.push(name);
            }
        }
    }
    out.sort();
    out
}

impl Icons {
    pub fn new(ctx: &egui::Context) -> Icons {
        let material: Vec<(&'static str, char)> = include_str!("../../assets/material.txt")
            .lines()
            .filter_map(|line| {
                let mut parts = line.split('\t');
                Some((parts.next()?, char::from_u32(parts.next()?.parse().ok()?)?))
            })
            .collect();
        let mut simple = vec![];
        let mut simple_map = HashMap::new();
        for line in include_str!("../../assets/simple.txt").lines() {
            let mut parts = line.split('\t');
            let (Some(name), Some(code)) = (parts.next(), parts.next()) else { continue };
            let Some(ch) = code.parse().ok().and_then(char::from_u32) else { continue };
            simple.push((name, ch));
            simple_map.insert(name, (ch, parts.next().and_then(hex)));
        }
        let (tx, rx) = mpsc::channel();
        let wake = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(apps::list());
            wake.request_repaint();
        });
        let (icon_tx, requests) = mpsc::sync_channel::<String>(32);
        let (results, icon_rx) = mpsc::channel();
        let wake = ctx.clone();
        std::thread::spawn(move || {
            while let Ok(id) = requests.recv() {
                let image = apps::icon(&id, 64).map(|img| {
                    egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.rgba)
                });
                if results.send((id, image)).is_err() {
                    break;
                }
                wake.request_repaint();
            }
        });
        Icons {
            material_map: material.iter().copied().collect(),
            simple_map,
            material,
            simple,
            textures: HashMap::new(),
            apps: vec![],
            apps_rx: Some(rx),
            app_map: HashMap::new(),
            icon_tx,
            icon_rx,
            pending: HashSet::new(),
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.apps_rx {
            if let Ok(list) = rx.try_recv() {
                self.apps = list;
                for (i, app) in self.apps.iter().enumerate() {
                    self.app_map.entry(app.name.clone()).or_insert(i);
                    self.app_map.entry(app.id.clone()).or_insert(i);
                }
                self.apps_rx = None;
            }
        }
        for _ in 0..8 {
            let Ok((id, image)) = self.icon_rx.try_recv() else { return };
            self.pending.remove(&id);
            let key = format!("app:{id}");
            let tex = image.map(|img| ctx.load_texture(&key, img, egui::TextureOptions::LINEAR));
            self.textures.insert(key, tex);
        }
        ctx.request_repaint();
    }

    fn system_texture(&mut self, id: &str) -> Option<TextureHandle> {
        if let Some(tex) = self.textures.get(&format!("app:{id}")) {
            return tex.clone();
        }
        if !self.pending.contains(id) && self.icon_tx.try_send(id.to_owned()).is_ok() {
            self.pending.insert(id.to_owned());
        }
        None
    }

    pub fn glyph(&self, name: &str) -> char {
        self.material_map.get(name).copied().unwrap_or('?')
    }

    fn texture(&mut self, ctx: &egui::Context, key: String, make: impl FnOnce() -> Option<egui::ColorImage>) -> Option<TextureHandle> {
        if let Some(t) = self.textures.get(&key) {
            return t.clone();
        }
        let t = make().map(|img| ctx.load_texture(&key, img, egui::TextureOptions::LINEAR));
        self.textures.insert(key, t.clone());
        t
    }

    pub fn image(&mut self, ctx: &egui::Context, key: &str, webp: &'static [u8]) -> Option<TextureHandle> {
        self.texture(ctx, format!("webp:{key}"), || {
            let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(webp)).ok()?;
            let (width, height) = decoder.dimensions();
            let mut pixels = vec![0; decoder.output_buffer_size()?];
            decoder.read_image(&mut pixels).ok()?;
            let size = [width as usize, height as usize];
            if decoder.has_alpha() {
                Some(egui::ColorImage::from_rgba_unmultiplied(size, &pixels))
            } else {
                Some(egui::ColorImage::from_rgb(size, &pixels))
            }
        })
    }

    pub fn svg_texture(&mut self, ctx: &egui::Context, key: &str, svg: &'static str, px: u32) -> Option<TextureHandle> {
        self.texture(ctx, format!("svg:{key}:{px}"), || svg_image(svg, px))
    }

    fn paint(painter: &Painter, tex: &TextureHandle, rect: Rect) {
        painter.image(tex.id(), rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
    }

    pub fn draw(&mut self, ctx: &egui::Context, painter: &Painter, rect: Rect, theme: &str, icon: &str, color: Color32) {
        let size = rect.width().min(rect.height());
        let center = rect.center();
        let text = |painter: &Painter, s: String, font: FontId, color: Color32| {
            painter.text(center, egui::Align2::CENTER_CENTER, s, font, color);
        };
        let inner = Rect::from_center_size(center, egui::vec2(size * 0.85, size * 0.85));

        match theme {
            "material-symbols-rounded" => {
                if let Some(&ch) = self.material_map.get(icon) {
                    text(painter, ch.to_string(), FontId::new(size * 0.75, FontFamily::Name("material".into())), color);
                    return;
                }
            }
            "simple-icons" | "simple-icons-colored" => {
                if let Some(&(ch, brand)) = self.simple_map.get(icon) {
                    let tint = if theme == "simple-icons-colored" { brand.unwrap_or(color) } else { color };
                    text(painter, ch.to_string(), FontId::new(size * 0.7, FontFamily::Name("simple".into())), tint);
                    return;
                }
            }
            "emoji" => {
                text(painter, icon.to_string(), FontId::proportional(size * 0.7), color);
                return;
            }
            "kando" => {
                let px = (size * ctx.pixels_per_point()).ceil().max(8.0) as u32;
                let key = format!("kando:{icon}:{px}:{:?}", color);
                let tex = self.texture(ctx, key, || {
                    let svg = ACTION_ICONS.iter().find(|(n, _)| *n == icon)?.1;
                    let hex = format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b());
                    svg_image(&svg.replace("currentColor", &hex), px)
                });
                if let Some(tex) = tex {
                    Self::paint(painter, &tex, Rect::from_center_size(center, egui::vec2(size * 0.75, size * 0.75)));
                    return;
                }
            }
            "system" => {
                let id = self.app_map.get(icon).map(|&i| self.apps[i].id.clone());
                if let Some(id) = id {
                    if let Some(tex) = self.system_texture(&id) {
                        Self::paint(painter, &tex, inner);
                        return;
                    }
                }
            }
            "base64" | "" => {}
            user => {
                let file = crate::data_dir().join("icon-themes").join(user).join(icon);
                let tex = self.texture(ctx, format!("file:{}", file.display()), || {
                    let bytes = std::fs::read(&file).ok()?;
                    if icon.to_lowercase().ends_with(".svg") {
                        return svg_image(&String::from_utf8_lossy(&bytes), 128);
                    }
                    let pm = tiny_skia::Pixmap::decode_png(&bytes).ok()?;
                    Some(egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data()))
                });
                if let Some(tex) = tex {
                    Self::paint(painter, &tex, inner);
                    return;
                }
            }
        }

        if let Some((_, data)) = icon.split_once("base64,") {
            let key = format!("b64:{}:{}", icon.len(), &icon[icon.len().saturating_sub(32)..]);
            let tex = self.texture(ctx, key, || {
                let bytes = STANDARD.decode(data.trim()).ok()?;
                let pm = tiny_skia::Pixmap::decode_png(&bytes).ok()?;
                Some(egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data()))
            });
            if let Some(tex) = tex {
                Self::paint(painter, &tex, Rect::from_center_size(center, egui::vec2(size * 0.9, size * 0.9)));
                return;
            }
        }

        let label: String = icon.chars().take(2).collect();
        text(painter, label, FontId::proportional(size * 0.4), color);
    }
}

pub fn svg_image(svg: &str, px: u32) -> Option<egui::ColorImage> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).ok()?;
    let mut pm = tiny_skia::Pixmap::new(px, px)?;
    let k = px as f32 / tree.size().width().max(tree.size().height());
    resvg::render(&tree, tiny_skia::Transform::from_scale(k, k), &mut pm.as_mut());
    Some(egui::ColorImage::from_rgba_premultiplied([px as usize, px as usize], pm.data()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Icons, mpsc::Receiver<String>, mpsc::Sender<(String, Option<egui::ColorImage>)>) {
        let (icon_tx, requests) = mpsc::sync_channel(1);
        let (results, icon_rx) = mpsc::channel();
        let icons = Icons {
            material: vec![],
            simple: vec![],
            material_map: HashMap::new(),
            simple_map: HashMap::new(),
            textures: HashMap::new(),
            apps: vec![],
            apps_rx: None,
            app_map: HashMap::new(),
            icon_tx,
            icon_rx,
            pending: HashSet::new(),
        };
        (icons, requests, results)
    }

    #[test]
    fn requests_once_and_caches_failure() {
        let (mut icons, requests, results) = setup();
        assert!(icons.system_texture("app").is_none());
        assert_eq!(requests.try_recv().unwrap(), "app");
        assert!(icons.system_texture("app").is_none());
        assert!(requests.try_recv().is_err());
        results.send(("app".into(), None)).unwrap();
        icons.poll(&egui::Context::default());
        assert!(icons.pending.is_empty());
        assert!(icons.system_texture("app").is_none());
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn full_queue_retries_without_waiting() {
        let (mut icons, requests, _results) = setup();
        icons.system_texture("first");
        icons.system_texture("second");
        assert!(!icons.pending.contains("second"));
        assert_eq!(requests.try_recv().unwrap(), "first");
        icons.system_texture("second");
        assert_eq!(requests.try_recv().unwrap(), "second");
        assert!(icons.pending.contains("second"));
    }

    #[test]
    fn uploads_in_batches_and_reuses_texture() {
        let (mut icons, _requests, results) = setup();
        let ctx = egui::Context::default();
        for i in 0..9 {
            let id = i.to_string();
            icons.pending.insert(id.clone());
            results.send((id, Some(egui::ColorImage::new([2, 2], Color32::WHITE)))).unwrap();
        }
        icons.poll(&ctx);
        assert_eq!(icons.textures.len(), 8);
        assert_eq!(icons.pending.len(), 1);
        icons.poll(&ctx);
        assert!(icons.pending.is_empty());
        let first = icons.system_texture("0").unwrap();
        assert_eq!(first.id(), icons.system_texture("0").unwrap().id());
        assert_eq!(icons.textures.len(), 9);
    }
}
