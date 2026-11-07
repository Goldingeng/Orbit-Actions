use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, Vec2};

pub struct Tone {
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub line: Color32,
    pub stage: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub button: Color32,
    pub hover: Color32,
}

pub fn colors(ctx: &egui::Context) -> Tone {
    if ctx.style().visuals.dark_mode {
        Tone {
            bg: Color32::from_gray(28),
            panel: Color32::from_gray(35),
            card: Color32::from_gray(51),
            line: Color32::from_gray(77),
            stage: Color32::from_gray(48),
            text: Color32::from_gray(245),
            dim: Color32::from_gray(190),
            button: Color32::from_gray(68),
            hover: Color32::from_gray(82),
        }
    } else {
        Tone {
            bg: Color32::from_gray(235),
            panel: Color32::from_gray(248),
            card: Color32::from_gray(230),
            line: Color32::from_gray(185),
            stage: Color32::from_gray(240),
            text: Color32::from_gray(30),
            dim: Color32::from_gray(80),
            button: Color32::from_gray(222),
            hover: Color32::from_gray(207),
        }
    }
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    let mut style = (*ctx.style()).clone();
    style.visuals = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    ctx.set_style(style);
    let tone = colors(ctx);
    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;
    v.panel_fill = tone.panel;
    v.window_fill = tone.panel;
    v.extreme_bg_color = tone.button;
    v.faint_bg_color = tone.card;
    v.hyperlink_color = tone.text;
    v.override_text_color = Some(tone.text);
    v.widgets.noninteractive.weak_bg_fill = tone.dim;
    v.selection.bg_fill = tone.hover;
    v.selection.stroke = Stroke::new(1.0, tone.text);
    v.window_corner_radius = CornerRadius::same(12);
    v.window_stroke = Stroke::new(1.0, tone.line);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.fg_stroke.color = tone.text;
        w.corner_radius = CornerRadius::same(6);
    }
    v.widgets.noninteractive.fg_stroke.color = tone.dim;
    v.widgets.inactive.bg_fill = tone.button;
    v.widgets.inactive.weak_bg_fill = tone.button;
    v.widgets.hovered.bg_fill = tone.hover;
    v.widgets.hovered.weak_bg_fill = tone.hover;
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(16.0));
    style.text_styles.insert(egui::TextStyle::Button, FontId::proportional(16.0));
    style.text_styles.insert(egui::TextStyle::Small, FontId::proportional(14.0));
    style.text_styles.insert(egui::TextStyle::Heading, FontId::proportional(22.0));
    ctx.set_style(style);
}

pub fn dialog(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = Vec2::new(12.0, 12.0);
    ui.spacing_mut().button_padding = Vec2::new(12.0, 8.0);
    ui.spacing_mut().interact_size.y = 36.0;
}
