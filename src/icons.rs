use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tiny_skia::Pixmap;

use crate::apps;

static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<Pixmap>>>>> = OnceLock::new();
static APPS: OnceLock<Vec<apps::App>> = OnceLock::new();

pub fn themes_dir() -> PathBuf {
    crate::menu::path().with_file_name("icon-themes")
}

fn from_rgba(img: apps::Image) -> Option<Pixmap> {
    let mut pm = Pixmap::new(img.width, img.height)?;
    for (d, s) in pm.data_mut().chunks_exact_mut(4).zip(img.rgba.chunks_exact(4)) {
        let a = s[3] as u32;
        d[0] = (s[0] as u32 * a / 255) as u8;
        d[1] = (s[1] as u32 * a / 255) as u8;
        d[2] = (s[2] as u32 * a / 255) as u8;
        d[3] = s[3];
    }
    Some(pm)
}

fn load(theme: &str, icon: &str) -> Option<Pixmap> {
    if theme == "system" {
        let list = APPS.get_or_init(apps::list);
        let app = list.iter().find(|a| a.name == icon || a.id == icon)?;
        return from_rgba(apps::icon(&app.id, 128)?);
    }
    let file = themes_dir().join(theme).join(icon);
    let bytes = std::fs::read(&file).ok()?;
    if icon.to_lowercase().ends_with(".svg") {
        return crate::draw::svg(&String::from_utf8_lossy(&bytes), 128.0, None);
    }
    Pixmap::decode_png(&bytes).ok()
}

pub fn image(theme: &str, icon: &str) -> Option<Arc<Pixmap>> {
    if icon.is_empty() {
        return None;
    }
    let key = format!("{theme}\u{1}{icon}");
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().unwrap().get(&key) {
        return hit.clone();
    }
    let value = load(theme, icon).map(Arc::new);
    let mut cache = cache.lock().unwrap();
    if cache.len() >= 256 {
        cache.clear();
    }
    cache.insert(key, value.clone());
    value
}

pub fn clear() {
    if let Some(cache) = CACHE.get() {
        cache.lock().unwrap().clear();
    }
}
