use serde_json::Value;
use std::collections::HashMap;

pub type Rgba = [f32; 4];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Style {
    Default,
    CleanCircle,
    NeonLights,
    Purity,
    RainbowLabels,
}

#[derive(Clone)]
pub struct Theme {
    pub style: Style,
    pub max_radius: f32,
    pub wrap_width: f32,
    pub center_text: bool,
    pub wedges: bool,
    pub separators: bool,
    pub colors: HashMap<String, Rgba>,
}

pub const THEMES: &[(&str, &str, &str)] = &[
    ("default", "Default", "Simon Schneegans"),
    ("clean-circle", "Clean Circle", "Simon Schneegans"),
    ("neon-lights", "Neon Lights", "Simon Schneegans"),
    ("purity", "Purity", "Luctarie"),
    ("rainbow-labels", "Rainbow Labels", "Simon Schneegans"),
];

pub fn default_colors(id: &str) -> &'static [(&'static str, &'static str)] {
    match id {
        "clean-circle" => &[
            ("action-icon-color", "rgba(255, 200, 225, 1)"),
            ("submenu-icon-color", "#333333"),
            ("text-color", "#222222"),
            ("background-color", "#ffffff"),
        ],
        "neon-lights" => &[
            ("glow-color", "rgba(255, 140, 210, 0.75)"),
            ("connector-color", "rgba(255, 200, 255, 0.75)"),
            ("separator-color", "rgba(255, 255, 255, 0.2)"),
            ("wedge-color", "rgba(255, 140, 210, 0.1)"),
        ],
        "purity" => &[
            ("text-color", "rgb(255 255 255 / 0.9)"),
            ("icon-color", "rgb(255 255 255 / 0.9)"),
            ("icon-hover-color", "rgb(255 255 255 / 1.0)"),
            ("ring-color", "rgb(13 13 13 / 0.86)"),
            ("inner-circle", "rgba(255, 255, 255, 0.4)"),
            ("wedge-color", "rgb(132 0 255 / 0.50)"),
            ("sector-color-submenu", "rgb(103 0 237 / 0.82)"),
            ("sector-color-action", "rgb(198 118 255 / 0.82)"),
            ("background-main", "rgb(0 0 0 / 0)"),
            ("background-secondary", "rgb(0 0 0 / 0.85)"),
        ],
        "rainbow-labels" => &[
            ("icon-color", "#333333"),
            ("label-background-color", "#333333"),
            ("label-color", "#dddddd"),
        ],
        _ => &[
            ("background-color", "rgb(255, 255, 255)"),
            ("text-color", "rgb(60, 60, 60)"),
            ("border-color", "rgb(109, 109, 109)"),
            ("hover-color", "rgb(255, 200, 200)"),
            ("wedge-highlight-color", "rgba(0, 0, 0, 0.1)"),
            ("wedge-color", "rgba(0, 0, 0, 0.2)"),
        ],
    }
}

pub fn presets(id: &str) -> &'static [(&'static str, &'static str)] {
    match id {
        "clean-circle" => &[
            ("Cotton Candy", include_str!("../assets/themes/clean-circle/Cotton Candy.json")),
            ("Dark and Cozy", include_str!("../assets/themes/clean-circle/Dark and Cozy.json")),
        ],
        "neon-lights" => &[
            ("Blue Glow", include_str!("../assets/themes/neon-lights/Blue Glow.json")),
            ("Green Glow", include_str!("../assets/themes/neon-lights/Green Glow.json")),
        ],
        "purity" => &[
            ("Electric", include_str!("../assets/themes/purity/Electric.json")),
            ("Orange", include_str!("../assets/themes/purity/Orange.json")),
            ("Pink", include_str!("../assets/themes/purity/Pink.json")),
        ],
        "rainbow-labels" => &[("Bright Prism", include_str!("../assets/themes/rainbow-labels/Bright Prism.json"))],
        _ => &[
            ("Blue Glass", include_str!("../assets/themes/default/Blue Glass.json")),
            ("Dark Night", include_str!("../assets/themes/default/Dark Night.json")),
        ],
    }
}

pub fn color(s: &str) -> Option<Rgba> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix('#') {
        let n = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok().map(|x| x as f32 / 255.0);
        let short = |i: usize| u8::from_str_radix(h.get(i..i + 1)?, 16).ok().map(|x| (x * 17) as f32 / 255.0);
        return match h.len() {
            3 => Some([short(0)?, short(1)?, short(2)?, 1.0]),
            6 => Some([n(0)?, n(2)?, n(4)?, 1.0]),
            8 => Some([n(0)?, n(2)?, n(4)?, n(6)?]),
            _ => None,
        };
    }
    let inner = s.split_once('(')?.1.trim_end_matches(')');
    let parts: Vec<f32> = inner
        .split(|c| c == ',' || c == '/' || c == ' ')
        .filter(|p| !p.is_empty())
        .filter_map(|p| match p.strip_suffix('%') {
            Some(x) => x.parse::<f32>().ok().map(|x| x / 100.0),
            None => p.parse::<f32>().ok(),
        })
        .collect();
    if parts.len() < 3 {
        return None;
    }
    Some([parts[0] / 255.0, parts[1] / 255.0, parts[2] / 255.0, *parts.get(3).unwrap_or(&1.0)])
}

pub fn system_dark() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize".encode_utf16().chain(Some(0)).collect();
    let name: Vec<u16> = "AppsUseLightTheme".encode_utf16().chain(Some(0)).collect();
    let mut value: u32 = 1;
    let mut size = 4u32;
    let ok = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), &mut value as *mut u32 as _, &mut size)
    };
    ok == 0 && value == 0
}

pub fn load(config: &Value) -> Theme {
    let dark = config["enableDarkModeForMenuThemes"].as_bool().unwrap_or(false) && system_dark();
    let (theme_key, colors_key) = if dark { ("darkMenuTheme", "darkMenuThemeColors") } else { ("menuTheme", "menuThemeColors") };
    let id = config[theme_key].as_str().unwrap_or("default");
    let id = if THEMES.iter().any(|t| t.0 == id) { id } else { "default" };

    let mut colors: HashMap<String, Rgba> = default_colors(id)
        .iter()
        .map(|(k, v)| (k.to_string(), color(v).unwrap_or([0.0, 0.0, 0.0, 1.0])))
        .collect();
    let user = &config[colors_key][id];
    if let Some(map) = user.as_object() {
        for (k, v) in map {
            if let Some(c) = v.as_str().and_then(color) {
                colors.insert(k.clone(), c);
            }
        }
    }

    let (style, max_radius, wrap_width, center_text, wedges, separators) = match id {
        "clean-circle" => (Style::CleanCircle, 160.0, 95.0, true, false, false),
        "neon-lights" => (Style::NeonLights, 160.0, 95.0, true, true, true),
        "purity" => (Style::Purity, 180.0, 110.0, true, true, false),
        "rainbow-labels" => (Style::RainbowLabels, 160.0, 90.0, false, false, false),
        _ => (Style::Default, 160.0, 95.0, true, true, false),
    };
    Theme { style, max_radius, wrap_width, center_text, wedges, separators, colors }
}

impl Theme {
    pub fn c(&self, name: &str) -> Rgba {
        self.colors.get(name).copied().unwrap_or([0.0, 0.0, 0.0, 0.0])
    }
}

pub fn hsl(h: f32, s: f32, l: f32, a: f32) -> Rgba {
    let h = h.rem_euclid(360.0) / 360.0;
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0), a]
}

pub fn to_hsl(c: Rgba) -> (f32, f32, f32) {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == c[0] {
        (c[1] - c[2]) / d + if c[1] < c[2] { 6.0 } else { 0.0 }
    } else if max == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    (h * 60.0, s, l)
}

pub fn mix(a: Rgba, b: Rgba, k: f32) -> Rgba {
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k, a[3] + (b[3] - a[3]) * k]
}

pub fn alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], c[3] * a]
}
