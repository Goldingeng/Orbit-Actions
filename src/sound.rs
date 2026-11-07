use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;
use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

static PLAYING: Mutex<Option<Vec<u8>>> = Mutex::new(None);

const BUILTIN: &str = r#"{
  "sounds": {
    "openMenu": { "file": "select.wav", "volume": 0.3 },
    "closeMenu": { "file": "final.wav", "volume": 0.5 },
    "openSubmenu": { "file": "select.wav", "volume": 0.4, "minPitch": 0.9, "maxPitch": 0.9 },
    "closeSubmenu": { "file": "select.wav", "volume": 0.4, "minPitch": 0.7, "maxPitch": 0.7 },
    "hoverParent": { "file": "hover.wav", "minPitch": 0.8, "maxPitch": 0.8 },
    "hoverButton": { "file": "hover.wav", "minPitch": 0.9, "maxPitch": 1.0 },
    "hoverSubmenu": { "file": "hover.wav", "minPitch": 0.9, "maxPitch": 0.9 }
  }
}"#;

pub struct Theme {
    id: String,
    description: Value,
    volume: f32,
}

pub fn dir() -> PathBuf {
    crate::data_dir().join("sound-themes")
}

pub fn json5(text: &str) -> Value {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == '"' || c == '\'' {
            out.push('"');
            i += 1;
            while i < chars.len() && chars[i] != c {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    out.push(chars[i]);
                    out.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if chars[i] == '"' {
                    out.push('\\');
                }
                out.push(chars[i]);
                i += 1;
            }
            out.push('"');
            i += 1;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let mut j = i;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if chars.get(j) == Some(&':') && !matches!(word.as_str(), "true" | "false" | "null") {
                out.push('"');
                out.push_str(&word);
                out.push('"');
            } else {
                out.push_str(&word);
            }
            continue;
        }
        if c == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if matches!(chars.get(j), Some('}') | Some(']')) {
                i += 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    serde_json::from_str(&out).unwrap_or(Value::Null)
}

pub fn installed() -> Vec<(String, String)> {
    let mut list = vec![("simple-clicks".to_string(), "Simple Clicks".to_string())];
    if let Ok(entries) = std::fs::read_dir(dir()) {
        for e in entries.flatten() {
            let id = e.file_name().to_string_lossy().to_string();
            let desc = json5(&std::fs::read_to_string(e.path().join("theme.json5")).unwrap_or_default());
            if desc.is_object() && id != "simple-clicks" {
                list.push((id.clone(), desc["name"].as_str().unwrap_or(&id).to_string()));
            }
        }
    }
    list
}

impl Theme {
    pub fn load(id: &str, volume: f32) -> Option<Theme> {
        if id.is_empty() || id == "none" {
            return None;
        }
        let description = if id == "simple-clicks" && !dir().join(id).exists() {
            serde_json::from_str(BUILTIN).ok()?
        } else {
            let v = json5(&std::fs::read_to_string(dir().join(id).join("theme.json5")).ok()?);
            if v.is_null() {
                return None;
            }
            v
        };
        Some(Theme { id: id.to_string(), description, volume })
    }

    fn file(&self, name: &str) -> Option<Vec<u8>> {
        if let Ok(bytes) = std::fs::read(dir().join(&self.id).join(name)) {
            return Some(bytes);
        }
        if self.id == "simple-clicks" {
            return Some(match name {
                "final.wav" => include_bytes!("../assets/sounds/simple-clicks/final.wav").to_vec(),
                "hover.wav" => include_bytes!("../assets/sounds/simple-clicks/hover.wav").to_vec(),
                "select.wav" => include_bytes!("../assets/sounds/simple-clicks/select.wav").to_vec(),
                _ => return None,
            });
        }
        None
    }

    pub fn play(&self, kind: &str) {
        let sound = &self.description["sounds"][kind];
        let Some(file) = sound["file"].as_str() else { return };
        let Some(mut wav) = self.file(file) else { return };
        let volume = sound["volume"].as_f64().unwrap_or(1.0) as f32 * self.volume;
        let lo = sound["minPitch"].as_f64().unwrap_or(1.0) as f32;
        let hi = sound["maxPitch"].as_f64().unwrap_or(1.0) as f32;
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let rate = lo + (hi - lo) * (seed % 1000) as f32 / 1000.0;
        if !adjust(&mut wav, volume, rate) {
            return;
        }
        let mut slot = PLAYING.lock().unwrap();
        unsafe { PlaySoundW(wav.as_ptr() as _, std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT) };
        *slot = Some(wav);
    }
}

fn adjust(wav: &mut [u8], volume: f32, rate: f32) -> bool {
    if wav.len() < 44 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return false;
    }
    let mut pos = 12;
    let mut bits = 16;
    while pos + 8 <= wav.len() {
        let id: [u8; 4] = [wav[pos], wav[pos + 1], wav[pos + 2], wav[pos + 3]];
        let size = u32::from_le_bytes([wav[pos + 4], wav[pos + 5], wav[pos + 6], wav[pos + 7]]) as usize;
        let body = pos + 8;
        if &id == b"fmt " && body + 16 <= wav.len() {
            bits = u16::from_le_bytes([wav[body + 14], wav[body + 15]]);
            let sample_rate = u32::from_le_bytes([wav[body + 4], wav[body + 5], wav[body + 6], wav[body + 7]]);
            let byte_rate = u32::from_le_bytes([wav[body + 8], wav[body + 9], wav[body + 10], wav[body + 11]]);
            let new_rate = (sample_rate as f32 * rate) as u32;
            let new_bytes = (byte_rate as f32 * rate) as u32;
            wav[body + 4..body + 8].copy_from_slice(&new_rate.to_le_bytes());
            wav[body + 8..body + 12].copy_from_slice(&new_bytes.to_le_bytes());
        }
        if &id == b"data" {
            let end = (body + size).min(wav.len());
            match bits {
                16 => {
                    for s in wav[body..end].chunks_exact_mut(2) {
                        let v = i16::from_le_bytes([s[0], s[1]]) as f32 * volume;
                        s.copy_from_slice(&(v.clamp(-32768.0, 32767.0) as i16).to_le_bytes());
                    }
                }
                8 => {
                    for s in wav[body..end].iter_mut() {
                        let v = (*s as f32 - 128.0) * volume + 128.0;
                        *s = v.clamp(0.0, 255.0) as u8;
                    }
                }
                _ => {}
            }
        }
        pos = body + size + (size & 1);
    }
    true
}
