use serde_json::Value;

#[path = "locales.rs"]
mod locales;

pub use locales::LOCALES;

pub struct Tr {
    main: Value,
    fallback: Value,
    own: Value,
    extra: Value,
}

const OVERRIDES: &str = include_str!("../assets/overrides.json");

fn system_languages() -> Vec<String> {
    use windows_sys::Win32::Globalization::{GetUserDefaultLocaleName, GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    let mut languages = vec![];
    let mut count = 0u32;
    let mut size = 0u32;
    unsafe {
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, std::ptr::null_mut(), &mut size) != 0 && size > 0 {
            let mut buf = vec![0u16; size as usize];
            if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, buf.as_mut_ptr(), &mut size) != 0 {
                languages.extend(buf.split(|&c| c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy));
            }
        }
        let mut buf = [0u16; 85];
        let n = GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32);
        if n > 1 {
            languages.push(String::from_utf16_lossy(&buf[..n as usize - 1]));
        }
    }
    languages
}

fn pick(lang: &str) -> Option<&'static (&'static str, &'static str)> {
    let lang = lang.replace('_', "-");
    LOCALES
        .iter()
        .find(|(code, _)| code.eq_ignore_ascii_case(&lang))
        .or_else(|| {
            let base = lang.split('-').next().unwrap_or("");
            let script = if lang.contains("TW") || lang.contains("HK") || lang.contains("Hant") { "zh-Hant" } else { "zh-Hans" };
            let wanted = if base == "zh" { script } else { base };
            LOCALES.iter().find(|(code, _)| code.eq_ignore_ascii_case(wanted))
        })
}

impl Tr {
    pub fn new(locale: &str) -> Tr {
        let en: Value = pick("en").and_then(|(_, t)| serde_json::from_str(t).ok()).unwrap_or_default();
        let found = if locale.is_empty() || locale == "auto" {
            system_languages().iter().find_map(|lang| pick(lang)).or_else(|| pick("en"))
        } else {
            pick(locale)
        };
        let main = found.and_then(|(_, t)| serde_json::from_str(t).ok()).unwrap_or_else(|| en.clone());
        let all: Value = serde_json::from_str(OVERRIDES).unwrap_or_default();
        let code = found.map(|(code, _)| code.to_lowercase()).unwrap_or_else(|| "en".into());
        let own = all[code.as_str()].clone();
        let extra = all["en"].clone();
        Tr { main, fallback: en, own, extra }
    }

    pub fn t(&self, key: &str) -> String {
        let find = |root: &Value| {
            key.split('.')
                .try_fold(root, |node, part| node.get(part))
                .and_then(|node| node.as_str())
                .map(|s| s.to_string())
        };
        if let Some(text) = self.own.get(key).and_then(|v| v.as_str()) {
            return text.to_string();
        }
        let text = find(&self.main)
            .or_else(|| self.extra.get(key).and_then(|v| v.as_str()).map(String::from))
            .or_else(|| find(&self.fallback))
            .unwrap_or_else(|| key.to_string());
        let mut text = plain(&text).replace("لكاندو", "لـ Orbit Actions");
        for name in ["Kando", "Кандо", "كاندو", "کاندو", "காண்டோ"] {
            text = text.replace(name, "Orbit Actions");
        }
        text
    }
}

fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](") else { break };
        let Some(end) = rest[open + close..].find(')') else { break };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..open + close]);
        rest = &rest[open + close + end + 1..];
    }
    out.push_str(rest);
    out.replace("**", "")
}
