use ab_glyph::{point, Font, FontRef, FontVec, GlyphId, PxScale, ScaleFont};
use std::collections::{HashMap, HashSet};
use std::cell::RefCell;
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use tiny_skia::*;

use crate::math::{self, v, V};
use crate::menu::{Item, Kind};
use crate::pie::{Class, Pie};
use crate::theme::{self, alpha, hsl, mix, Rgba, Style, Theme};

static TEXT: OnceLock<Option<FontVec>> = OnceLock::new();
static BOLD: OnceLock<Option<FontVec>> = OnceLock::new();
static EMOJI: OnceLock<Option<FontVec>> = OnceLock::new();
static MATERIAL: OnceLock<(FontRef<'static>, HashMap<&'static str, u32>)> = OnceLock::new();
static SIMPLE: OnceLock<(FontRef<'static>, HashMap<&'static str, (u32, Option<Rgba>)>)> = OnceLock::new();

type MaskKey = (u32, u32, i32, Vec<u8>);

#[derive(Default)]
struct MaskCache {
    masks: HashMap<MaskKey, Arc<[u8]>>,
    bytes: usize,
}

thread_local! {
    static MASKS: RefCell<MaskCache> = RefCell::new(MaskCache::default());
}

#[cfg(test)]
mod tests {
    use super::*;
fn blur_reference(pm: &mut Pixmap, radius: f32) {
    let r = radius.round() as i32;
    if r < 1 {
        return;
    }
    let w = pm.width() as i32;
    let h = pm.height() as i32;
    let mut buf: Vec<f32> = pm.data().iter().map(|b| *b as f32).collect();
    let mut tmp = buf.clone();
    let norm = 1.0 / (2 * r + 1) as f32;
    for _ in 0..3 {
        for y in 0..h {
            for ch in 0..4 {
                let at = |x: i32| ((y * w + x.clamp(0, w - 1)) * 4 + ch) as usize;
                let mut acc: f32 = (-r..=r).map(|x| buf[at(x)]).sum();
                for x in 0..w {
                    tmp[at(x)] = acc * norm;
                    acc += buf[at(x + r + 1)] - buf[at(x - r)];
                }
            }
        }
        for x in 0..w {
            for ch in 0..4 {
                let at = |y: i32| ((y.clamp(0, h - 1) * w + x) * 4 + ch) as usize;
                let mut acc: f32 = (-r..=r).map(|y| tmp[at(y)]).sum();
                for y in 0..h {
                    buf[at(y)] = acc * norm;
                    acc += tmp[at(y + r + 1)] - tmp[at(y - r)];
                }
            }
        }
    }
    for (b, f) in pm.data_mut().iter_mut().zip(buf) {
        *b = f.clamp(0.0, 255.0) as u8;
    }
}


    #[test]
    fn blur_mask() {
        for (w, h) in [(3, 2), (31, 25)] {
            for radius in [0.0, 1.0, 3.0, 8.0] {
                let mut source = Pixmap::new(w, h).unwrap();
                for (i, px) in source.data_mut().chunks_exact_mut(4).enumerate() {
                    let a = ((i * 71 + 19) % 256) as u8;
                    px.copy_from_slice(&[a / 2, a / 3, a / 4, a]);
                }
                let mut expected = source.clone();
                blur_reference(&mut expected, radius);
                for _ in 0..2 {
                    let mut actual = source.clone();
                    blur(&mut actual, radius);
                    let alpha = |pm: &Pixmap| pm.data().chunks_exact(4).map(|px| px[3]).collect::<Vec<_>>();
                    assert_eq!(alpha(&actual), alpha(&expected));
                }
            }
        }
    }


    fn root() -> Item {
        Item {
            name: "Пример меню".into(), icon: "apps".into(), theme: "material-symbols-rounded".into(),
            img: None, kind: Kind::Root, angle: None, select: vec![], open: vec![], activate: vec![], hover: vec![],
            key: None, activate_key: None, children: vec![], remote: false,
        }
    }

    #[test]
    fn cursor_target() {
        let mut settings = crate::menu::settings();
        settings.warp = true;
        settings.theme = theme::load(&serde_json::json!({"menuTheme": "purity"}));
        for fixed in [None, Some((0.02, 0.02)), Some((0.5, 0.5))] {
            let pie = Pie::new(&root(), &settings, v(800.0, 600.0), v(10.0, 12.0), fixed, false, false);
            let targets: Vec<V> = pie.events.iter().filter_map(|event| match event {
                crate::pie::Event::Warp(target) => Some(*target),
                _ => None,
            }).collect();
            assert_eq!(targets.len(), 1);
            assert!(math::len(targets[0] - pie.center_position()) < 0.01);
            assert!(targets[0].x >= settings.theme.max_radius);
            assert!(targets[0].y >= settings.theme.max_radius);
        }
    }

    #[test]
    fn settled_frame() {
        let mut settings = crate::menu::settings();
        settings.theme = theme::load(&serde_json::json!({"menuTheme": "purity"}));
        let pie = Pie::new(&root(), &settings, v(800.0, 600.0), v(400.0, 300.0), None, false, false);
        let mut renderer = Renderer::new(settings.theme.clone(), 1.0, true);
        renderer.area = (800, 600);
        renderer.quick_keys = true;
        renderer.born = Instant::now() - std::time::Duration::from_secs(5);
        let first = renderer.render(&pie).unwrap();
        let pixels = first.pixmap.data().to_vec();
        renderer.recycle(first.pixmap);
        let next = renderer.render(&pie).unwrap();
        assert_eq!(pixels, next.pixmap.data());
        assert!(!renderer.busy);
    }

    #[test]
    #[ignore]
    fn render_bench() {
        warm();
        let item = |icon: &str| Item {
            name: "Пример меню".into(), icon: icon.into(), theme: "material-symbols-rounded".into(),
            img: None, kind: Kind::Button, angle: None, select: vec![], open: vec![], activate: vec![], hover: vec![],
            key: None, activate_key: None, children: vec![], remote: false,
        };
        let mut root = item("apps");
        root.kind = Kind::Root;
        root.children = ["folder", "language", "play_arrow", "settings", "keyboard", "content_copy", "arrow_back", "arrow_forward"].iter().map(|icon| item(icon)).collect();
        for (w, h, scale) in [(1920, 1080, 1.0), (3840, 2160, 1.5)] {
            for (id, _, _) in theme::THEMES {
                let mut settings = crate::menu::settings();
                settings.theme = theme::load(&serde_json::json!({"menuTheme": id}));
                let mut pie = Pie::new(&root, &settings, v(w as f32 / scale, h as f32 / scale), v(w as f32 / scale / 2.0, h as f32 / scale / 2.0), None, false, false);
                let mut renderer = Renderer::new(settings.theme.clone(), scale, true);
                renderer.area = (w, h);
                renderer.born = Instant::now() - std::time::Duration::from_secs(4);
                renderer.fade_to(1.0, 0.0);
                for _ in 0..3 {
                    let frame = renderer.render(&pie).unwrap();
                    renderer.recycle(frame.pixmap);
                }
                for moving in [false, true] {
                    let mut samples = vec![];
                    for i in 0..24 {
                        if moving {
                            pie.motion(pie.center_position() + math::dir(i as f32 * 15.0, 120.0), false);
                        }
                        let at = Instant::now();
                        let frame = std::hint::black_box(renderer.render(&pie).unwrap());
                        renderer.recycle(frame.pixmap);
                        samples.push(at.elapsed().as_secs_f64() * 1000.0);
                    }
                    samples.sort_by(f64::total_cmp);
                    println!("{id} {w}x{h} moving={moving} median={:.2} p95={:.2} ms", samples[12], samples[22]);
                }
            }
        }
    }
}

fn system_font(name: &str) -> Option<FontVec> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let bytes = std::fs::read(format!("{windir}\\Fonts\\{name}")).ok()?;
    FontVec::try_from_vec(bytes).ok()
}

fn text_font() -> Option<&'static FontVec> {
    TEXT.get_or_init(|| system_font("segoeui.ttf").or_else(|| system_font("arial.ttf"))).as_ref()
}

fn bold_font() -> Option<&'static FontVec> {
    BOLD.get_or_init(|| system_font("segoeuib.ttf").or_else(|| system_font("arialbd.ttf"))).as_ref().or_else(text_font)
}

fn material() -> &'static (FontRef<'static>, HashMap<&'static str, u32>) {
    MATERIAL.get_or_init(|| {
        let font = FontRef::try_from_slice(include_bytes!("../assets/material.ttf")).unwrap();
        let names = include_str!("../assets/material.txt")
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .filter_map(|(n, id)| Some((n, id.parse().ok()?)))
            .collect();
        (font, names)
    })
}

fn simple() -> &'static (FontRef<'static>, HashMap<&'static str, (u32, Option<Rgba>)>) {
    SIMPLE.get_or_init(|| {
        let font = FontRef::try_from_slice(include_bytes!("../assets/simple.ttf")).unwrap();
        let names = include_str!("../assets/simple.txt")
            .lines()
            .filter_map(|l| {
                let mut parts = l.split('\t');
                let name = parts.next()?;
                let code = parts.next()?.parse().ok()?;
                let color = parts.next().and_then(theme::color);
                Some((name, (code, color)))
            })
            .collect();
        (font, names)
    })
}

pub fn warm() {
    let _ = text_font();
    let _ = material();
    let _ = simple();
}

type Curve = (f32, f32, f32, f32);
const OVERSHOOT: Curve = (0.775, 1.325, 0.535, 1.0);
const EASE: Curve = (0.25, 0.1, 0.25, 1.0);
const EASE_OUT: Curve = (0.0, 0.0, 0.58, 1.0);
const SLOW_FADE: Curve = (0.25, 0.5, 0.35, 1.0);
const VIGNETTE: Curve = (0.22, 1.0, 0.36, 1.0);

#[derive(Clone, Copy)]
struct Timing(f32, Curve);

#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    start: f32,
    timing: Timing,
}

impl Tween {
    fn get(&self, now: f32) -> f32 {
        if self.timing.0 <= 0.0 {
            return self.to;
        }
        let t = ((now - self.start) / self.timing.0).clamp(0.0, 1.0);
        let (a, b, c, d) = self.timing.1;
        self.from + (self.to - self.from) * math::bezier(a, b, c, d, t)
    }

    fn running(&self, now: f32) -> bool {
        now < self.start + self.timing.0 && (self.from - self.to).abs() > 1e-4
    }
}

type Job = Box<dyn FnOnce(&mut Pixmap, V)>;

struct Canvas {
    jobs: Vec<Job>,
    lo: V,
    hi: V,
}

impl Canvas {
    fn grow(&mut self, c: V, r: f32) {
        self.lo = v(self.lo.x.min(c.x - r), self.lo.y.min(c.y - r));
        self.hi = v(self.hi.x.max(c.x + r), self.hi.y.max(c.y + r));
    }

    fn add(&mut self, c: V, r: f32, job: impl FnOnce(&mut Pixmap, V) + 'static) {
        self.grow(c, r);
        self.jobs.push(Box::new(job));
    }
}

pub struct Frame {
    pub pixmap: Pixmap,
    pub x: i32,
    pub y: i32,
    pub alpha: f32,
}

pub struct Renderer {
    tweens: HashMap<(usize, &'static str), Tween>,
    seen: HashSet<(usize, &'static str)>,
    born: Instant,
    fade: Tween,
    opened: f32,
    vignette: Option<Pixmap>,
    spare: Option<Pixmap>,
    pub busy: bool,
    pub snap: bool,
    pub animate: bool,
    pub theme: Theme,
    pub scale: f32,
    pub origin: (i32, i32),
    pub area: (i32, i32),
    pub wedges: bool,
    pub quick_keys: bool,
    pub underline: bool,
    pub reactive: bool,
}

fn rgba(c: Rgba) -> Color {
    Color::from_rgba(c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0), c[3].clamp(0.0, 1.0)).unwrap_or(Color::TRANSPARENT)
}

fn solid(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(rgba(c));
    p.anti_alias = true;
    p
}

fn disk(pm: &mut Pixmap, c: V, r: f32, color: Rgba) {
    if r <= 0.0 || color[3] <= 0.0 {
        return;
    }
    if let Some(path) = PathBuilder::from_circle(c.x, c.y, r) {
        pm.fill_path(&path, &solid(color), FillRule::Winding, Transform::identity(), None);
    }
}

fn circle_line(pm: &mut Pixmap, c: V, r: f32, width: f32, color: Rgba) {
    if r <= 0.0 || color[3] <= 0.0 {
        return;
    }
    if let Some(path) = PathBuilder::from_circle(c.x, c.y, r) {
        let stroke = Stroke { width, ..Default::default() };
        pm.stroke_path(&path, &solid(color), &stroke, Transform::identity(), None);
    }
}

fn radial(pm: &mut Pixmap, c: V, r: f32, stops: &[(f32, Rgba)], clip: Option<&Path>) {
    if r <= 0.5 {
        return;
    }
    let stops: Vec<GradientStop> = stops.iter().map(|(t, col)| GradientStop::new(t.clamp(0.0, 1.0), rgba(*col))).collect();
    let Some(shader) = RadialGradient::new(Point::from_xy(c.x, c.y), Point::from_xy(c.x, c.y), r, stops, SpreadMode::Pad, Transform::identity()) else { return };
    let mut paint = Paint::default();
    paint.shader = shader;
    paint.anti_alias = true;
    match clip {
        Some(path) => pm.fill_path(path, &paint, FillRule::EvenOdd, Transform::identity(), None),
        None => {
            if let Some(path) = PathBuilder::from_circle(c.x, c.y, r) {
                pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
            }
        }
    }
}

fn shadow(pm: &mut Pixmap, c: V, r: f32, offset: V, blur: f32, color: Rgba) {
    let at = c + offset;
    let outer = r + blur * 1.5;
    let t = |d: f32| (d / outer).clamp(0.0, 1.0);
    let k = |a: f32| alpha(color, a);
    radial(
        pm,
        at,
        outer,
        &[
            (0.0, k(1.0)),
            (t(r - blur), k(0.98)),
            (t(r - blur * 0.5), k(0.84)),
            (t(r), k(0.5)),
            (t(r + blur * 0.5), k(0.16)),
            (t(r + blur), k(0.02)),
            (1.0, k(0.0)),
        ],
        None,
    );
}

fn arc_points(pb: &mut PathBuilder, c: V, r: f32, a0: f32, a1: f32, first: bool) {
    let steps = (((a1 - a0).abs() / 3.0).ceil() as usize).max(2);
    for i in 0..=steps {
        let a = a0 + (a1 - a0) * i as f32 / steps as f32;
        let p = c + math::dir(a, r);
        if i == 0 && first {
            pb.move_to(p.x, p.y);
        } else {
            pb.line_to(p.x, p.y);
        }
    }
}

fn sector(c: V, r0: f32, r1: f32, a0: f32, a1: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    arc_points(&mut pb, c, r1, a0, a1, true);
    if r0 > 0.0 {
        arc_points(&mut pb, c, r0, a1, a0, false);
    } else {
        pb.line_to(c.x, c.y);
    }
    pb.close();
    pb.finish()
}

fn annulus(c: V, r0: f32, r1: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.push_circle(c.x, c.y, r1);
    pb.push_circle(c.x, c.y, r0);
    pb.finish()
}

fn fill(pm: &mut Pixmap, path: &Path, color: Rgba) {
    if color[3] > 0.0 {
        pm.fill_path(path, &solid(color), FillRule::EvenOdd, Transform::identity(), None);
    }
}

fn blur(pm: &mut Pixmap, radius: f32) {
    let r = radius.round() as i32;
    if r < 1 {
        return;
    }
    let key = (pm.width(), pm.height(), r, pm.data().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>());
    if let Some(mask) = MASKS.with(|cache| cache.borrow().masks.get(&key).cloned()) {
        for (px, a) in pm.data_mut().chunks_exact_mut(4).zip(mask.iter()) {
            px[3] = *a;
        }
        return;
    }
    let w = key.0 as i32;
    let h = key.1 as i32;
    let mut buf: Vec<f32> = key.3.iter().map(|b| *b as f32).collect();
    let mut tmp = buf.clone();
    let norm = 1.0 / (2 * r + 1) as f32;
    for _ in 0..3 {
        for y in 0..h {
                let at = |x: i32| (y * w + x.clamp(0, w - 1)) as usize;
                let mut acc: f32 = (-r..=r).map(|x| buf[at(x)]).sum();
                for x in 0..w {
                    tmp[at(x)] = acc * norm;
                    acc += buf[at(x + r + 1)] - buf[at(x - r)];
                }
        }
        for x in 0..w {
                let at = |y: i32| (y.clamp(0, h - 1) * w + x) as usize;
                let mut acc: f32 = (-r..=r).map(|y| tmp[at(y)]).sum();
                for y in 0..h {
                    buf[at(y)] = acc * norm;
                    acc += tmp[at(y + r + 1)] - tmp[at(y - r)];
                }
        }
    }
    let mask: Arc<[u8]> = buf.into_iter().map(|f| f.clamp(0.0, 255.0) as u8).collect();
    for (px, a) in pm.data_mut().chunks_exact_mut(4).zip(mask.iter()) {
        px[3] = *a;
    }
    MASKS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let bytes = key.3.len() + mask.len();
        if bytes > 4 * 1024 * 1024 { return; }
        if cache.bytes + bytes > 4 * 1024 * 1024 || cache.masks.len() >= 256 {
            cache.masks.clear();
            cache.bytes = 0;
        }
        cache.bytes += bytes;
        cache.masks.insert(key, mask);
    });
}

fn glow(pm: &mut Pixmap, c: V, size: f32, radius: f32, color: Rgba, offset: V, draw: &dyn Fn(&mut Pixmap, V)) {
    if color[3] <= 0.01 {
        return;
    }
    let pad = radius * 2.5 + 2.0;
    let side = (size + pad * 2.0).ceil().max(1.0) as u32;
    let Some(mut tmp) = Pixmap::new(side, side) else { return };
    let mid = v(side as f32 / 2.0, side as f32 / 2.0);
    draw(&mut tmp, mid);
    blur(&mut tmp, radius / 2.0);
    for px in tmp.data_mut().chunks_exact_mut(4) {
        let a = px[3] as f32 / 255.0 * color[3];
        px[0] = (color[0] * a * 255.0) as u8;
        px[1] = (color[1] * a * 255.0) as u8;
        px[2] = (color[2] * a * 255.0) as u8;
        px[3] = (a * 255.0) as u8;
    }
    let at = c + offset - mid;
    pm.draw_pixmap(at.x.round() as i32, at.y.round() as i32, tmp.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
}

fn glyph_run<F: Font>(pm: &mut Pixmap, font: &F, ids: &[GlyphId], c: V, size: f32, color: Rgba, underline: Option<usize>) {
    let scaled = font.as_scaled(PxScale::from(size));
    let width: f32 = ids.iter().map(|&id| scaled.h_advance(id)).sum();
    let baseline = c.y + (scaled.ascent() + scaled.descent()) / 2.0;
    let mut x = c.x - width / 2.0;
    let w = pm.width() as i32;
    let h = pm.height() as i32;

    for (i, &id) in ids.iter().enumerate() {
        let advance = scaled.h_advance(id);
        if underline == Some(i) {
            if let Some(rect) = Rect::from_xywh(x, baseline + size * 0.12, advance, (size / 16.0).max(1.0)) {
                pm.fill_rect(rect, &solid(color), Transform::identity(), None);
            }
        }
        let glyph = id.with_scale_and_position(size, point(x, baseline));
        x += advance;
        let Some(outline) = font.outline_glyph(glyph) else { continue };
        let bounds = outline.px_bounds();
        let data = pm.data_mut();
        outline.draw(|gx, gy, cov| {
            let px = bounds.min.x as i32 + gx as i32;
            let py = bounds.min.y as i32 + gy as i32;
            if px < 0 || py < 0 || px >= w || py >= h {
                return;
            }
            let a = cov.min(1.0) * color[3];
            let i = ((py * w + px) * 4) as usize;
            for k in 0..3 {
                data[i + k] = (color[k] * 255.0 * a + data[i + k] as f32 * (1.0 - a)) as u8;
            }
            data[i + 3] = (255.0 * a + data[i + 3] as f32 * (1.0 - a)) as u8;
        });
    }
}

fn text_width(font: &FontVec, text: &str, size: f32) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    text.chars().map(|ch| scaled.h_advance(font.glyph_id(ch))).sum()
}

fn text(pm: &mut Pixmap, font: &FontVec, s: &str, c: V, size: f32, color: Rgba, underline: Option<usize>) {
    let ids: Vec<GlyphId> = s.chars().map(|ch| font.glyph_id(ch)).collect();
    glyph_run(pm, font, &ids, c, size, color, underline);
}

pub fn action_svg(name: &str, size: f32, color: Rgba) -> Option<Pixmap> {
    let file = ACTION_ICONS.iter().find(|(n, _)| *n == name)?.1;
    svg(file, size, Some(color))
}

pub fn svg(source: &str, size: f32, color: Option<Rgba>) -> Option<Pixmap> {
    let text = match color {
        Some(c) => source.replace("currentColor", &format!("#{:02x}{:02x}{:02x}", (c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)),
        None => source.to_string(),
    };
    let tree = resvg::usvg::Tree::from_str(&text, &resvg::usvg::Options::default()).ok()?;
    let side = size.ceil().max(1.0) as u32;
    let mut pm = Pixmap::new(side, side)?;
    let k = size / tree.size().width().max(tree.size().height());
    resvg::render(&tree, Transform::from_scale(k, k), &mut pm.as_mut());
    if let Some(c) = color {
        if c[3] < 1.0 {
            for px in pm.data_mut().iter_mut() {
                *px = (*px as f32 * c[3]) as u8;
            }
        }
    }
    Some(pm)
}

fn blit(pm: &mut Pixmap, img: &Pixmap, c: V, size: f32, opacity: f32, clip: Option<f32>) {
    let k = size / img.width().max(img.height()) as f32;
    let tx = c.x - img.width() as f32 * k / 2.0;
    let ty = c.y - img.height() as f32 * k / 2.0;
    let mask = clip.and_then(|r| {
        let mut m = Mask::new(pm.width(), pm.height())?;
        let path = PathBuilder::from_circle(c.x, c.y, r)?;
        m.fill_path(&path, FillRule::Winding, true, Transform::identity());
        Some(m)
    });
    let paint = PixmapPaint { opacity, quality: FilterQuality::Bicubic, ..Default::default() };
    pm.draw_pixmap(0, 0, img.as_ref(), &paint, Transform::from_row(k, 0.0, 0.0, k, tx, ty), mask.as_ref());
}

pub fn draw_icon(pm: &mut Pixmap, item: &Item, c: V, box_size: f32, color: Rgba, opacity: f32, round: bool) {
    let r = box_size / 2.0;
    if opacity <= 0.01 || r <= 0.5 {
        return;
    }
    let color = alpha(color, opacity);
    if let Some(img) = &item.img {
        blit(pm, img, c, box_size, opacity, round.then_some(r));
        return;
    }
    let em = box_size * 0.75;
    match item.theme.as_str() {
        "material-symbols-rounded" => {
            let (font, names) = material();
            if let Some(ch) = names.get(item.icon.as_str()).and_then(|&cp| char::from_u32(cp)) {
                glyph_run(pm, font, &[font.glyph_id(ch)], c, em, color, None);
                return;
            }
        }
        "simple-icons" | "simple-icons-colored" => {
            let (font, names) = simple();
            if let Some(&(cp, brand)) = names.get(item.icon.as_str()) {
                if let Some(ch) = char::from_u32(cp) {
                    let tint = match (item.theme.as_str(), brand) {
                        ("simple-icons-colored", Some(b)) => alpha(b, opacity),
                        _ => color,
                    };
                    glyph_run(pm, font, &[font.glyph_id(ch)], c, em, tint, None);
                    return;
                }
            }
        }
        "kando" => {
            if let Some(svg) = action_svg(&item.icon, em, color) {
                blit(pm, &svg, c, em, 1.0, None);
                return;
            }
        }
        "emoji" => {
            if let Some(font) = EMOJI.get_or_init(|| system_font("seguiemj.ttf")).as_ref() {
                let ids: Vec<GlyphId> = item.icon.chars().filter(|&ch| ch != '\u{fe0f}' && ch != '\u{200d}').map(|ch| font.glyph_id(ch)).collect();
                glyph_run(pm, font, &ids, c, em, color, None);
                return;
            }
        }
        _ => {}
    }
    if let Some(img) = crate::icons::image(&item.theme, &item.icon) {
        blit(pm, &img, c, box_size, opacity, round.then_some(r));
        return;
    }
    if let Some(font) = text_font() {
        let label = if !item.icon.is_empty() && item.icon.chars().count() <= 2 { item.icon.clone() } else { item.name.chars().take(1).collect() };
        text(pm, font, &label, c, em * 0.8, color, None);
    }
}

fn wrap(font: &FontVec, text: &str, size: f32, d: f32) -> Option<Vec<String>> {
    let measure = |s: &str| text_width(font, s, size);
    let line_h = size * 1.2;
    let r = d / 2.0;
    for count in 1..=6usize {
        let top = (d - line_h * count as f32) / 2.0;
        if top < -line_h {
            return None;
        }
        let room = |i: usize| -> f32 {
            let a = top + line_h * i as f32 - r;
            let b = a + line_h;
            let edge = a.abs().max(b.abs()).min(r);
            2.0 * (r * r - edge * edge).max(0.0).sqrt()
        };
        let mut lines: Vec<String> = vec![String::new()];
        let mut ok = true;
        for word in text.split(' ') {
            let line = lines.last().unwrap();
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if measure(&candidate) <= room(lines.len() - 1) {
                *lines.last_mut().unwrap() = candidate;
                continue;
            }
            if lines.len() >= count {
                ok = false;
                break;
            }
            if measure(word) > room(lines.len()) {
                let mut piece = String::new();
                for ch in word.chars() {
                    let next = format!("{piece}{ch}");
                    if measure(&next) > room(lines.len()) && !piece.is_empty() {
                        let done = std::mem::take(&mut piece);
                        if lines.last().unwrap().is_empty() {
                            *lines.last_mut().unwrap() = done;
                        } else {
                            lines.push(done);
                        }
                        if lines.len() >= count {
                            ok = false;
                            break;
                        }
                        piece = ch.to_string();
                    } else {
                        piece = next;
                    }
                }
                if !ok {
                    break;
                }
                lines.push(piece);
            } else {
                lines.push(word.to_string());
            }
        }
        if ok && lines.len() <= count {
            return Some(lines.into_iter().filter(|l| !l.is_empty()).collect());
        }
    }
    None
}

fn center_text(pm: &mut Pixmap, label: &str, key: Option<&str>, c: V, d: f32, max: f32, color: Rgba, glow_color: Option<Rgba>) {
    let Some(font) = text_font() else { return };
    let mut size = max;
    let mut lines = None;
    while size >= max * 0.7 {
        lines = wrap(font, label, size, d);
        if lines.is_some() {
            break;
        }
        size -= 1.0;
    }
    let size = size.max(max * 0.7);
    let lines = lines.unwrap_or_else(|| vec![label.to_string()]);

    let mut mark = key.filter(|k| k.chars().count() == 1).map(|k| k.to_lowercase());
    let line_h = size * 1.2;
    let top = c.y - line_h * lines.len() as f32 / 2.0;
    let mut placed = vec![];
    for (i, line) in lines.iter().enumerate() {
        let mut underline = None;
        if let Some(k) = &mark {
            if let Some(pos) = line.chars().position(|ch| ch.to_lowercase().to_string() == *k) {
                underline = Some(pos);
                mark = None;
            }
        }
        placed.push((line.clone(), v(c.x, top + line_h * (i as f32 + 0.5)), underline));
    }
    if let Some(g) = glow_color {
        let block = d.max(size * 6.0);
        let lines = placed.clone();
        glow(pm, c, block, 10.0 * size / 16.0, g, v(0.0, 0.0), &move |tmp, mid| {
            for (line, at, _) in &lines {
                text(tmp, font, line, *at - c + mid, size, [1.0, 1.0, 1.0, 1.0], None);
            }
        });
    }
    for (line, at, underline) in placed {
        text(pm, font, &line, at, size, color, underline);
    }
}

pub const ACTION_ICONS: &[(&str, &str)] = &[
    ("clipboard-item.svg", include_str!("../assets/action-icons/clipboard-item.svg")),
    ("close-menu-item.svg", include_str!("../assets/action-icons/close-menu-item.svg")),
    ("close-submenu-item.svg", include_str!("../assets/action-icons/close-submenu-item.svg")),
    ("command-item.svg", include_str!("../assets/action-icons/command-item.svg")),
    ("delay-item.svg", include_str!("../assets/action-icons/delay-item.svg")),
    ("edit-submenu.svg", include_str!("../assets/action-icons/edit-submenu.svg")),
    ("file-item.svg", include_str!("../assets/action-icons/file-item.svg")),
    ("focus-window-item.svg", include_str!("../assets/action-icons/focus-window-item.svg")),
    ("hotkey-item.svg", include_str!("../assets/action-icons/hotkey-item.svg")),
    ("inhibit-shortcuts-item.svg", include_str!("../assets/action-icons/inhibit-shortcuts-item.svg")),
    ("macro-item.svg", include_str!("../assets/action-icons/macro-item.svg")),
    ("redirect-item.svg", include_str!("../assets/action-icons/redirect-item.svg")),
    ("settings-item.svg", include_str!("../assets/action-icons/settings-item.svg")),
    ("submenu-item.svg", include_str!("../assets/action-icons/submenu-item.svg")),
    ("text-item.svg", include_str!("../assets/action-icons/text-item.svg")),
    ("uri-item.svg", include_str!("../assets/action-icons/uri-item.svg")),
    ("workflow-activate.svg", include_str!("../assets/action-icons/workflow-activate.svg")),
    ("workflow-hover.svg", include_str!("../assets/action-icons/workflow-hover.svg")),
    ("workflow-select.svg", include_str!("../assets/action-icons/workflow-select.svg")),
];

const NEON_ARROW: &str = include_str!("../assets/themes/neon-lights/arrow.svg");

struct Node {
    n: usize,
    class: Class,
    kind: Kind,
    hovered: bool,
    clicked: bool,
    dragged: bool,
    angle: f32,
    parent_angle: Option<f32>,
    side: u8,
    item: Item,
    key: String,
}

impl Renderer {
    pub fn new(theme: Theme, scale: f32, animate: bool) -> Renderer {
        Renderer {
            tweens: HashMap::new(),
            seen: HashSet::new(),
            born: Instant::now(),
            fade: Tween { from: 0.0, to: 0.0, start: 0.0, timing: Timing(0.075, EASE) },
            opened: 0.0,
            vignette: None,
            spare: None,
            busy: false,
            snap: true,
            animate,
            theme,
            scale,
            origin: (0, 0),
            area: (1920, 1080),
            wedges: false,
            quick_keys: false,
            underline: true,
            reactive: true,
        }
    }

    fn now(&self) -> f32 {
        self.born.elapsed().as_secs_f32()
    }

    pub fn fade_to(&mut self, to: f32, ms: f32) {
        let now = self.now();
        if to > 0.0 {
            self.opened = now;
        }
        self.fade = Tween { from: self.fade.get(now), to, start: now, timing: Timing(ms / 1000.0, EASE) };
    }

    pub fn faded_out(&self) -> bool {
        self.fade.to == 0.0 && !self.fade.running(self.now())
    }

    fn tw(&mut self, now: f32, n: usize, slot: &'static str, target: f32, timing: Timing) -> f32 {
        let key = (n, slot);
        self.seen.insert(key);
        let snap = self.snap || !self.animate;
        let t = self.tweens.entry(key).or_insert(Tween { from: target, to: target, start: now, timing });
        if snap {
            *t = Tween { from: target, to: target, start: now, timing };
        } else if (t.to - target).abs() > 1e-4 {
            *t = Tween { from: t.get(now), to: target, start: now, timing };
        }
        t.get(now)
    }

    fn tw_color(&mut self, now: f32, n: usize, slots: [&'static str; 4], c: Rgba, timing: Timing) -> Rgba {
        [
            self.tw(now, n, slots[0], c[0], timing),
            self.tw(now, n, slots[1], c[1], timing),
            self.tw(now, n, slots[2], c[2], timing),
            self.tw(now, n, slots[3], c[3], timing),
        ]
    }

    fn move_timing(&self) -> Timing {
        match self.theme.style {
            Style::Default | Style::NeonLights => Timing(0.25, OVERSHOOT),
            Style::CleanCircle | Style::RainbowLabels => Timing(0.2, EASE),
            Style::Purity => Timing(0.0, EASE),
        }
    }

    fn fade_timing(&self) -> Timing {
        match self.theme.style {
            Style::CleanCircle | Style::RainbowLabels => Timing(0.2, EASE),
            Style::Purity => Timing(0.1, EASE),
            _ => Timing(0.25, EASE),
        }
    }

    fn node(&self, pie: &Pie, n: usize, class: Class) -> Node {
        let node = &pie.nodes[n];
        let d = math::dir(node.angle, 1.0);
        let side = if d.x < -0.2 {
            0
        } else if d.x > 0.2 {
            1
        } else if d.y < 0.0 {
            2
        } else {
            3
        };
        let level = node.path.len();
        Node {
            n,
            class,
            kind: node.item.kind,
            hovered: pie.hovered == Some(n),
            clicked: pie.clicked == Some(n),
            dragged: pie.dragged == Some(n),
            angle: node.angle,
            parent_angle: if level > 1 { node.parent.map(|p| pie.nodes[p].angle) } else { None },
            side,
            item: node.item.clone(),
            key: node.key.clone(),
        }
    }

    fn child_hovered(pie: &Pie) -> bool {
        pie.hovered.is_some_and(|h| h != pie.center && pie.nodes[h].parent == Some(pie.center))
    }

    fn mid_angle(pie: &Pie, n: usize) -> f32 {
        let (s, e) = pie.nodes[n].wedge;
        (s + e) / 2.0
    }

    pub fn render(&mut self, pie: &Pie) -> Option<Frame> {
        let now = self.now();
        self.seen.clear();
        let s = self.scale;
        let mut cv = Canvas { jobs: vec![], lo: v(f32::MAX, f32::MAX), hi: v(f32::MIN, f32::MIN) };
        let center = pie.center_position();
        let screen = v(self.area.0 as f32, self.area.1 as f32);
        let mut fullscreen = false;
        let mut backdrop = None;

        if self.theme.style == Style::Purity {
            fullscreen = true;
            backdrop = self.purity_backdrop(now, screen);
        }
        if self.wedges && self.theme.wedges {
            if self.theme.style == Style::Default {
                fullscreen = true;
            }
            self.selection_wedges(pie, center * s, &mut cv);
        }
        if self.wedges && self.theme.separators {
            self.separators(pie, center * s, &mut cv);
        }

        self.walk(pie, 0, v(0.0, 0.0), 1.0, now, &mut cv);

        if let Some(n) = pie.text.filter(|_| self.theme.center_text) {
            let item = pie.nodes[n].item.clone();
            let key = if self.underline { Some(pie.nodes[n].key.clone()) } else { None };
            let d = self.theme.wrap_width * s;
            let c = center * s;
            let th = self.theme.clone();
            cv.add(c, d, move |pm, shift| {
                let c = c + shift;
                match th.style {
                    Style::NeonLights => center_text(pm, &item.name, key.as_deref(), c, d, 16.0 * s, [1.0, 1.0, 1.0, 1.0], Some(th.c("glow-color"))),
                    Style::Purity => {
                        disk(pm, c, 55.0 * s, mix(th.c("ring-color"), [0.0, 0.0, 0.0, 0.25], 0.5));
                        center_text(pm, &item.name, key.as_deref(), c, d * 0.9, 16.0 * s, th.c("text-color"), None);
                    }
                    _ => center_text(pm, &item.name, key.as_deref(), c, d, 16.0 * s, th.c("text-color"), None),
                }
            });
        }

        let seen = std::mem::take(&mut self.seen);
        self.tweens.retain(|k, _| seen.contains(k));
        self.seen = seen;
        self.snap = !self.animate;

        let (x0, y0, w, h) = if fullscreen {
            (0, 0, self.area.0.max(1) as u32, self.area.1.max(1) as u32)
        } else {
            cv.grow(center * s, 1.0);
            let x0 = cv.lo.x.floor() as i32;
            let y0 = cv.lo.y.floor() as i32;
            (x0, y0, (cv.hi.x.ceil() as i32 - x0).max(1) as u32, (cv.hi.y.ceil() as i32 - y0).max(1) as u32)
        };
        let mut pm = if let Some(pm) = backdrop {
            pm
        } else if let Some(mut pm) = self.spare.take().filter(|pm| pm.width() == w && pm.height() == h) {
            pm.fill(Color::TRANSPARENT);
            pm
        } else {
            Pixmap::new(w, h)?
        };
        let shift = v(-x0 as f32, -y0 as f32);
        for job in cv.jobs {
            job(&mut pm, shift);
        }

        let spinning = self.animate && self.theme.style == Style::NeonLights;
        let intro = self.animate && self.theme.style == Style::Purity && now - self.opened < 3.5;
        let keys = self.quick_keys && now - self.opened < 1.2 && matches!(self.theme.style, Style::CleanCircle | Style::Purity);
        self.busy = spinning || intro || keys || self.tweens.values().any(|t| t.running(now)) || self.fade.running(now);
        Some(Frame { pixmap: pm, x: self.origin.0 + x0, y: self.origin.1 + y0, alpha: self.fade.get(now) })
    }

    pub fn recycle(&mut self, pixmap: Pixmap) {
        self.spare = Some(pixmap);
    }

    fn walk(&mut self, pie: &Pie, n: usize, origin: V, cum: f32, now: f32, cv: &mut Canvas) {
        let class = pie.class(n);
        if class == Class::Hidden {
            return;
        }
        if self.theme.style == Style::Purity && class == Class::Grandchild {
            return;
        }
        let node = self.node(pie, n, class);
        let (offset, scale) = self.place(pie, &node, now);
        let at = origin + offset * cum;
        let my = cum * scale;

        if matches!(class, Class::Active | Class::Parent) && self.theme.style != Style::Purity {
            self.connector(pie, &node, at, my, now, cv);
        }

        let children = pie.nodes[n].children.clone();
        if self.theme.style == Style::RainbowLabels {
            self.rainbow_label(&node, at, my, now, cv);
            for &c in children.iter() {
                self.walk(pie, c, at, my, now, cv);
            }
            self.layers(pie, &node, at, my, now, cv);
        } else if self.theme.style == Style::Purity {
            self.layers(pie, &node, at, my, now, cv);
            for &c in children.iter().rev() {
                self.walk(pie, c, at, my, now, cv);
            }
        } else {
            for &c in children.iter().rev() {
                self.walk(pie, c, at, my, now, cv);
            }
            self.layers(pie, &node, at, my, now, cv);
        }
    }

    fn place(&mut self, pie: &Pie, node: &Node, now: f32) -> (V, f32) {
        let n = node.n;
        let style = self.theme.style;
        let (offset, scale) = match node.class {
            Class::Active | Class::Parent => (pie.nodes[n].pos.unwrap_or_default(), 1.0),
            Class::Child => {
                if node.dragged {
                    (pie.nodes[n].pos.unwrap_or_default(), 1.0)
                } else if style == Style::Purity {
                    (math::dir(Self::mid_angle(pie, n), 110.0), 1.0)
                } else {
                    let siblings = pie.nodes[pie.center].children.len() as f32;
                    let base_d: f32 = if style == Style::RainbowLabels { 80.0 } else { 100.0 };
                    let base = math::dir(node.angle, base_d.max(10.0 * siblings));
                    let pointer = if self.reactive { pie.input.angle } else { (node.angle + 180.0) % 360.0 };
                    let mut diff = (node.angle - pointer).abs() % 360.0;
                    diff = diff.min(360.0 - diff);
                    let hovered_child = Self::child_hovered(pie);
                    let k = match style {
                        Style::Default if hovered_child => {
                            if node.hovered {
                                1.15
                            } else {
                                1.15 - (diff / 180.0).powf(0.25) * 0.15
                            }
                        }
                        Style::NeonLights if hovered_child => {
                            if node.hovered {
                                1.5
                            } else {
                                1.5 - (diff / 180.0).powf(0.25) * 0.5
                            }
                        }
                        Style::CleanCircle => {
                            if node.hovered {
                                1.2
                            } else if node.kind == Kind::Submenu && hovered_child {
                                1.1 - (diff / 180.0).powf(0.2) * 0.1
                            } else {
                                1.0
                            }
                        }
                        _ => 1.0,
                    };
                    (base * k, k)
                }
            }
            _ => {
                let d = match style {
                    Style::NeonLights | Style::RainbowLabels => 16.0,
                    _ => 25.0,
                };
                (math::dir(node.angle, d), 1.0)
            }
        };
        let timing = if node.dragged { Timing(0.0, EASE) } else { self.move_timing() };
        let x = self.tw(now, n, "x", offset.x, timing);
        let y = self.tw(now, n, "y", offset.y, timing);
        let k = self.tw(now, n, "scale", scale, timing);
        (v(x, y), k)
    }

    fn connector(&mut self, pie: &Pie, node: &Node, at: V, my: f32, now: f32, cv: &mut Canvas) {
        let n = node.n;
        let (reach, turn) = pie.nodes[n].connector;
        let timing = if pie.dragged.is_some() { Timing(0.0, EASE) } else { self.move_timing() };
        let length = self.tw(now, n, "reach", reach, timing) * my;
        let angle = self.tw(now, n, "turn", turn, timing);
        let style = self.theme.style;

        let visible = match style {
            Style::CleanCircle | Style::RainbowLabels => {
                let special = [pie.dragged, pie.clicked].iter().flatten().any(|&c| pie.nodes[c].parent == Some(n) && pie.kind(c) == Kind::Submenu);
                if node.class == Class::Parent || special {
                    1.0
                } else {
                    0.0
                }
            }
            _ => 1.0,
        };
        let opacity = self.tw(now, n, "conn-alpha", visible, self.fade_timing());
        if length < 0.5 || opacity <= 0.01 {
            return;
        }
        let s = self.scale;
        let th = self.theme.clone();
        let from = at * s;
        let length = length * s;
        let class = node.class;
        let hovered = node.hovered;
        cv.grow(from + math::dir(angle, length), 20.0 * s);
        cv.add(from, 20.0 * s, move |pm, shift| {
            let from = from + shift;
            let d = math::dir(angle, 1.0);
            let normal = v(-d.y, d.x);
            let bar = |a: V, width: f32| {
                let n = normal * (width / 2.0);
                let b = a + d * length;
                let mut pb = PathBuilder::new();
                pb.move_to(a.x + n.x, a.y + n.y);
                pb.line_to(b.x + n.x, b.y + n.y);
                pb.line_to(b.x - n.x, b.y - n.y);
                pb.line_to(a.x - n.x, a.y - n.y);
                pb.close();
                pb.finish()
            };
            match th.style {
                Style::Default => {
                    let color = if class == Class::Active {
                        th.c("hover-color")
                    } else if hovered {
                        mix(th.c("hover-color"), th.c("border-color"), 0.5)
                    } else {
                        th.c("border-color")
                    };
                    if let Some(p) = bar(from, 10.0 * my * s) {
                        fill(pm, &p, color);
                    }
                }
                Style::CleanCircle | Style::RainbowLabels => {
                    let color = if th.style == Style::CleanCircle { th.c("background-color") } else { th.c("label-background-color") };
                    let width = 10.0 * my * s;
                    glow(pm, from + d * (length / 2.0), length + width, 10.0 * s, [0.0, 0.0, 0.0, 0.3 * opacity], v(0.0, 0.0), &|tmp, mid| {
                        if let Some(p) = bar(mid - d * (length / 2.0), width) {
                            fill(tmp, &p, [1.0, 1.0, 1.0, 1.0]);
                        }
                    });
                    if let Some(p) = bar(from, width) {
                        fill(pm, &p, alpha(color, opacity));
                    }
                }
                Style::NeonLights => {
                    let glow_color = th.c("glow-color");
                    let spacing = 14.0 * s;
                    let fade = 80.0 * s;
                    let count = (length / spacing) as i32;
                    let r = 2.0 * s;
                    for i in 0..=count {
                        let t = i as f32 * spacing + (length - count as f32 * spacing) / 2.0;
                        let k = if t < fade || t > length - fade {
                            0.0
                        } else if t < length / 2.0 {
                            ((t - fade) / (length / 2.0 - fade).max(1.0)).clamp(0.0, 1.0)
                        } else {
                            ((length - fade - t) / (length / 2.0 - fade).max(1.0)).clamp(0.0, 1.0)
                        };
                        if k > 0.01 {
                            let p = from + d * t;
                            glow(pm, p, r * 2.0, 6.0 * s, alpha(glow_color, k), v(0.0, 0.0), &|tmp, mid| disk(tmp, mid, r, [1.0, 1.0, 1.0, 1.0]));
                            disk(pm, p, r, [1.0, 1.0, 1.0, k]);
                        }
                    }
                }
                Style::Purity => {}
            }
        });
    }

    fn layers(&mut self, pie: &Pie, node: &Node, at: V, my: f32, now: f32, cv: &mut Canvas) {
        match self.theme.style {
            Style::Default => self.default_layers(pie, node, at, my, now, cv),
            Style::CleanCircle => self.clean_layers(pie, node, at, my, now, cv),
            Style::NeonLights => self.neon_layers(pie, node, at, my, now, cv),
            Style::RainbowLabels => self.rainbow_layers(pie, node, at, my, now, cv),
            Style::Purity => self.purity_layers(pie, node, at, now, cv),
        }
    }

    fn default_layers(&mut self, pie: &Pie, node: &Node, at: V, my: f32, now: f32, cv: &mut Canvas) {
        let n = node.n;
        let th = self.theme.clone();
        let child_hovered = Self::child_hovered(pie);
        let size = match node.class {
            Class::Active => 100.0,
            Class::Grandchild => 15.0,
            _ => 50.0,
        };
        let layer = match node.class {
            Class::Active if child_hovered => 1.1,
            _ if node.hovered && node.clicked => 0.9,
            _ => 1.0,
        };
        let target = match node.class {
            Class::Grandchild => th.c("border-color"),
            _ if node.hovered => th.c("hover-color"),
            _ => th.c("background-color"),
        };
        let icon_target = match node.class {
            Class::Active if child_hovered => 0.0,
            Class::Grandchild => 0.0,
            _ => 1.0,
        };
        let mt = self.move_timing();
        let size = self.tw(now, n, "size", size, mt) * self.tw(now, n, "layer", layer, mt);
        let bg = self.tw_color(now, n, ["r", "g", "b", "a"], target, mt);
        let icon = self.tw(now, n, "icon", icon_target, self.fade_timing());

        let s = self.scale;
        let c = at * s;
        let r = size * my * s / 2.0;
        let shadow_on = node.class != Class::Grandchild;
        let item = node.item.clone();
        let border = th.c("border-color");
        let text_color = th.c("text-color");
        cv.add(c, r + 18.0 * s, move |pm, shift| {
            let c = c + shift;
            if shadow_on {
                shadow(pm, c, r, v(2.0, 2.0) * s, 10.0 * s, [0.0, 0.0, 0.0, 0.5]);
            }
            disk(pm, c, r, bg);
            circle_line(pm, c, (r - s / 2.0).max(0.1), s.min(r), border);
            draw_icon(pm, &item, c, r * 1.8, text_color, icon, true);
        });

        if self.quick_keys && node.class == Class::Child && !node.dragged {
            let kc = c + math::dir(node.angle, -26.0 * my * s);
            let key = node.key.clone();
            let fill_c = if node.hovered { th.c("background-color") } else { th.c("hover-color") };
            cv.add(kc, 14.0 * s, move |pm, shift| {
                let kc = kc + shift;
                shadow(pm, kc, 10.0 * s, v(1.0, 1.0) * s, 6.0 * s, [0.0, 0.0, 0.0, 0.4]);
                disk(pm, kc, 10.0 * s, fill_c);
                circle_line(pm, kc, 9.5 * s, s, border);
                if let Some(f) = bold_font() {
                    text(pm, f, &key, kc, 13.0 * s, text_color, None);
                }
            });
        }
    }

    fn clean_layers(&mut self, pie: &Pie, node: &Node, at: V, my: f32, now: f32, cv: &mut Canvas) {
        let n = node.n;
        let th = self.theme.clone();
        let mt = self.move_timing();
        let child_hovered = Self::child_hovered(pie);
        let is_menu = node.kind != Kind::Button;
        let size = match node.class {
            Class::Active => 100.0,
            Class::Parent => 50.0,
            Class::Child if is_menu => 50.0,
            Class::Child => 60.0,
            _ => 15.0,
        };
        let layer = if node.hovered && node.clicked { 0.8 } else { 1.0 };
        let visible = if node.class == Class::Grandchild { 0.0 } else { 1.0 };
        let icon_target = match node.class {
            Class::Active if child_hovered => 0.0,
            Class::Grandchild => 0.0,
            _ => 1.0,
        };
        let size = self.tw(now, n, "size", size, mt) * self.tw(now, n, "layer", layer, mt);
        let opacity = self.tw(now, n, "alpha", visible, mt);
        let mut icon = self.tw(now, n, "icon", icon_target, mt);

        let mut key_alpha = 0.0;
        if self.quick_keys && node.class == Class::Child && !child_hovered {
            let t = ((now - self.opened) / 4.0).fract();
            let ramp = |a: f32, b: f32| ((t - a) / (b - a)).clamp(0.0, 1.0);
            key_alpha = if t < 0.18 {
                0.0
            } else if t < 0.25 {
                ramp(0.18, 0.25)
            } else if t < 0.57 {
                1.0
            } else if t < 0.65 {
                1.0 - ramp(0.57, 0.65)
            } else {
                0.0
            };
            icon *= 1.0 - key_alpha;
        }

        let s = self.scale;
        let c = at * s;
        let bg = th.c("background-color");

        if node.class == Class::Active {
            let hovered_angle = pie.hovered.filter(|&h| h != pie.center && Some(h) != pie.nodes[pie.center].parent).map(|h| pie.nodes[h].angle);
            let last = self.tweens.get(&(n, "arrow")).map(|t| t.to).unwrap_or(pie.input.angle);
            let show = hovered_angle.is_some() || pie.clicked.is_some();
            let target = if pie.clicked.is_some() { closest(pie.input.angle, last) } else { closest(hovered_angle.unwrap_or(pie.input.angle), last) };
            let angle = self.tw(now, n, "arrow", target, mt);
            let arrow_alpha = self.tw(now, n, "arrow-alpha", if show { 1.0 } else { 0.0 }, mt);
            let arrow_scale = self.tw(now, n, "arrow-scale", if show { 1.0 } else { 0.8 }, mt);
            if arrow_alpha > 0.01 {
                let k = arrow_scale * s;
                cv.add(c, 70.0 * s, move |pm, shift| {
                    let c = c + shift;
                    let mid = c + math::dir(angle, 54.0 * k);
                    let apex = mid + math::dir(angle, 5.0 * k);
                    let base = mid - math::dir(angle, 5.0 * k);
                    let side = math::dir(angle + 90.0, 10.0 * k);
                    let mut pb = PathBuilder::new();
                    pb.move_to(apex.x, apex.y);
                    pb.line_to(base.x + side.x, base.y + side.y);
                    pb.line_to(base.x - side.x, base.y - side.y);
                    pb.close();
                    if let Some(p) = pb.finish() {
                        fill(pm, &p, alpha(bg, arrow_alpha));
                    }
                });
            }
        }

        if opacity <= 0.01 {
            return;
        }
        let r = size * my * s / 2.0;
        let icon_color = if is_menu { th.c("submenu-icon-color") } else { th.c("action-icon-color") };
        let item = node.item.clone();
        let key = node.key.clone();
        let round = is_menu || node.class == Class::Active;
        cv.add(c, r + 20.0 * s, move |pm, shift| {
            let c = c + shift;
            if round {
                shadow(pm, c, r, v(0.0, 5.0) * s, 10.0 * s, [0.0, 0.0, 0.0, 0.3 * opacity]);
                disk(pm, c, r, alpha(bg, opacity));
                draw_icon(pm, &item, c, r * 1.8, icon_color, icon * opacity, true);
            } else {
                let it = item.clone();
                glow(pm, c, r * 2.0, 5.0 * s, [0.0, 0.0, 0.0, 0.5 * icon * opacity], v(0.0, 3.0) * s, &move |tmp, mid| {
                    draw_icon(tmp, &it, mid, r * 2.0, [1.0, 1.0, 1.0, 1.0], 1.0, false)
                });
                draw_icon(pm, &item, c, r * 2.0, icon_color, icon * opacity, false);
            }
            if key_alpha > 0.0 {
                if let Some(f) = bold_font() {
                    let size = if round { 22.0 } else { 28.0 } * s;
                    text(pm, f, &key.to_uppercase(), c, size, alpha(icon_color, key_alpha), None);
                }
            }
        });
    }

    fn neon_layers(&mut self, pie: &Pie, node: &Node, at: V, my: f32, now: f32, cv: &mut Canvas) {
        let n = node.n;
        let th = self.theme.clone();
        let mt = self.move_timing();
        let ft = self.fade_timing();
        let child_hovered = Self::child_hovered(pie);
        let size = match node.class {
            Class::Active => 100.0,
            Class::Grandchild => 15.0,
            _ => 50.0,
        };
        let layer = if node.hovered && node.clicked && matches!(node.class, Class::Parent | Class::Child) { 0.8 } else { 1.0 };
        let visible = if node.class == Class::Grandchild { 0.0 } else { 1.0 };
        let icon_target = if matches!(node.class, Class::Parent | Class::Child) { 1.0 } else { 0.0 };
        let size = self.tw(now, n, "size", size, mt) * self.tw(now, n, "layer", layer, mt);
        let opacity = self.tw(now, n, "alpha", visible, mt);
        let icon = self.tw(now, n, "icon", icon_target, ft);
        let s = self.scale;
        let c = at * s;
        let r = size * my * s / 2.0;
        let glow_color = th.c("glow-color");

        if node.class == Class::Active {
            let spin_slow = ((now - self.opened) / 10.0).fract() * 360.0;
            let spin_fast = ((now - self.opened) / 2.0).fract() * 360.0;
            let slow = self.tw(now, n, "ring-slow", if child_hovered { 0.0 } else { 1.0 }, mt);
            let fast = self.tw(now, n, "ring-fast", if child_hovered { 1.0 } else { 0.0 }, mt);
            let clicked = pie.clicked == Some(n);
            let slow_scale = self.tw(now, n, "ring-slow-scale", if clicked { 1.5 } else { 1.0 }, mt);
            let any_hovered = pie.hovered.is_some_and(|h| h != pie.center);
            let hovered_angle = pie.hovered.filter(|&h| h != pie.center).map(|h| {
                if Some(h) == pie.nodes[pie.center].parent {
                    (pie.nodes[pie.center].angle + 180.0) % 360.0
                } else {
                    pie.nodes[h].angle
                }
            });
            let last = self.tweens.get(&(n, "arrow")).map(|t| t.to).unwrap_or(pie.input.angle);
            let dragging = pie.dragged.is_some();
            let target = if dragging { closest(pie.input.angle, last) } else { closest(hovered_angle.unwrap_or(pie.input.angle), last) };
            let arrow = self.tw(now, n, "arrow", target, if dragging { Timing(0.0, EASE) } else { mt });
            let shown = any_hovered || dragging;
            let arrow_alpha = self.tw(now, n, "arrow-alpha", if shown { 1.0 } else { 0.0 }, mt);
            let arrow_scale = self.tw(now, n, "arrow-scale", if shown { 1.6 } else { 1.0 }, mt);
            let base = 100.0 * my * s;
            cv.add(c, base * 1.3, move |pm, shift| {
                let c = c + shift;
                if fast > 0.01 {
                    radial(pm, c, base * 0.5, &[(0.0, [0.0, 0.0, 0.0, 0.3 * fast]), (0.85, [0.0, 0.0, 0.0, 0.3 * fast]), (1.0, [0.0, 0.0, 0.0, 0.0])], None);
                    neon_ring(pm, c, base * 2.0, spin_fast, glow_color, true, fast);
                }
                if slow > 0.01 {
                    neon_ring(pm, c, base * slow_scale, spin_slow, glow_color, clicked, slow);
                }
                if arrow_alpha > 0.01 {
                    if let Some(img) = svg(NEON_ARROW, base * arrow_scale, None) {
                        let tx = c.x - img.width() as f32 / 2.0;
                        let ty = c.y - img.height() as f32 / 2.0;
                        let paint = PixmapPaint { opacity: arrow_alpha, quality: FilterQuality::Bicubic, ..Default::default() };
                        let tr = Transform::from_rotate_at(arrow, c.x, c.y).pre_translate(tx, ty);
                        pm.draw_pixmap(0, 0, img.as_ref(), &paint, tr, None);
                    }
                }
            });
        }

        if opacity <= 0.01 {
            return;
        }
        let item = node.item.clone();
        cv.add(c, r + 24.0 * s, move |pm, shift| {
            let c = c + shift;
            radial(pm, c, r * 2.0 * 0.53, &[(0.0, [0.0, 0.0, 0.0, 0.2 * opacity]), (1.0, [0.0, 0.0, 0.0, 0.0])], None);
            if icon > 0.01 {
                let it = item.clone();
                let draw = move |tmp: &mut Pixmap, mid: V| draw_icon(tmp, &it, mid, r * 2.0, [1.0, 1.0, 1.0, 1.0], 1.0, false);
                glow(pm, c, r * 2.0, 10.0 * s, alpha(glow_color, icon * opacity), v(0.0, 0.0), &draw);
                glow(pm, c, r * 2.0, 3.0 * s, [1.0, 1.0, 1.0, icon * opacity], v(0.0, 0.0), &draw);
                draw_icon(pm, &item, c, r * 2.0, [1.0, 1.0, 1.0, 1.0], icon * opacity, false);
            }
        });

        if self.quick_keys && node.class == Class::Child && !node.dragged {
            let kc = c + math::dir(node.angle, -36.0 * s);
            let key = node.key.clone();
            cv.add(kc, 30.0 * s, move |pm, shift| {
                let kc = kc + shift;
                radial(pm, kc, 18.0 * s, &[(0.0, [0.0, 0.0, 0.0, 0.1]), (0.75, [0.0, 0.0, 0.0, 0.0])], None);
                if let Some(f) = bold_font() {
                    let k = key.clone();
                    glow(pm, kc, 40.0 * s, 20.0 * s, glow_color, v(0.0, 0.0), &move |tmp, mid| text(tmp, f, &k, mid, 14.0 * s, [1.0, 1.0, 1.0, 1.0], None));
                    text(pm, f, &key, kc, 14.0 * s, [1.0, 1.0, 1.0, 1.0], None);
                }
            });
        }
    }

    fn rainbow_label(&mut self, node: &Node, at: V, my: f32, now: f32, cv: &mut Canvas) {
        let n = node.n;
        let visible = if node.class == Class::Child { 1.0 } else { 0.0 };
        let opacity = self.tw(now, n, "label", visible, Timing(0.2, EASE));
        if opacity <= 0.01 {
            return;
        }
        let Some(font) = text_font() else { return };
        let th = self.theme.clone();
        let s = self.scale * my;
        let c = at * self.scale;
        let name = node.item.name.clone();
        let key = node.key.clone();
        let quick = self.quick_keys;
        let underline = self.underline;
        let side = node.side;
        let angle = node.angle;
        let border = if node.hovered && node.clicked {
            [1.0, 1.0, 1.0, 1.0]
        } else if node.hovered {
            hsl(node.parent_angle.unwrap_or(0.0), 0.5, 0.85, 1.0)
        } else {
            hsl(angle, 0.5, 0.6, 1.0)
        };
        let inset = node.hovered;
        let icon = 40.0 * s;
        let size = icon / 2.7;
        let pad = icon / 3.0;
        let key_w = if quick { text_width(font, &key, size * 1.2) + 8.0 * s } else { 0.0 };
        let name_w = text_width(font, &name, size);
        let label_w = name_w + key_w;
        let (x0, x1, y0) = match side {
            1 => (c.x - icon / 2.0, c.x + icon / 2.0 + 5.0 * s + label_w + pad, c.y - icon / 2.0),
            0 => (c.x - icon / 2.0 - 5.0 * s - label_w - pad, c.x + icon / 2.0, c.y - icon / 2.0),
            2 => (c.x - (label_w + 2.0 * pad) / 2.0, c.x + (label_w + 2.0 * pad) / 2.0, c.y - 0.4 * icon - icon),
            _ => (c.x - (label_w + 2.0 * pad) / 2.0, c.x + (label_w + 2.0 * pad) / 2.0, c.y + 0.4 * icon),
        };
        let bg = th.c("label-background-color");
        let fg = th.c("label-color");
        let bbox = v((x0 + x1) / 2.0, y0 + icon / 2.0);
        cv.add(bbox, (x1 - x0) / 2.0 + icon, move |pm, shift| {
            let (x0, x1, y0) = (x0 + shift.x, x1 + shift.x, y0 + shift.y);
            let pill = |inset: f32| {
                let r = icon / 2.0 - inset;
                let (a, b, top, bottom) = (x0 + inset, x1 - inset, y0 + inset, y0 + icon - inset);
                let mut pb = PathBuilder::new();
                pb.move_to(a + r, top);
                pb.line_to(b - r, top);
                arc_points(&mut pb, v(b - r, top + r), r, 0.0, 180.0, false);
                pb.line_to(a + r, bottom);
                arc_points(&mut pb, v(a + r, top + r), r, 180.0, 360.0, false);
                pb.close();
                pb.finish()
            };
            if let Some(p) = pill(0.0) {
                fill(pm, &p, alpha(border, opacity));
            }
            if let Some(p) = pill(2.0 * s) {
                fill(pm, &p, alpha(bg, opacity));
            }
            if inset {
                if let Some(p) = pill(2.0 * s) {
                    fill(pm, &p, alpha(hsl(angle, 0.8, 0.8, 0.15), opacity));
                }
                if let Some(p) = pill(7.0 * s) {
                    fill(pm, &p, alpha(bg, opacity * 0.85));
                }
            }
            let mid_y = y0 + icon / 2.0;
            let mut x = match side {
                1 => x0 + icon + 5.0 * s,
                0 => x1 - icon - 5.0 * s - label_w,
                _ => x0 + pad,
            };
            let key_color = alpha(hsl(angle, 0.5, 0.6, 1.0), opacity);
            if quick && side == 0 {
                if let Some(bf) = bold_font() {
                    text(pm, bf, &key, v(x + (key_w - 8.0 * s) / 2.0, mid_y), size * 1.2, key_color, None);
                }
                x += key_w;
            }
            let mark = if underline { name.to_lowercase().chars().position(|ch| ch.to_string() == key.to_lowercase()) } else { None };
            text(pm, font, &name, v(x + name_w / 2.0, mid_y), size, alpha(fg, opacity), mark);
            x += name_w;
            if quick && side != 0 {
                if let Some(bf) = bold_font() {
                    text(pm, bf, &key, v(x + 8.0 * s + (key_w - 8.0 * s) / 2.0, mid_y), size * 1.2, key_color, None);
                }
            }
        });
    }

    fn rainbow_layers(&mut self, pie: &Pie, node: &Node, at: V, my: f32, now: f32, cv: &mut Canvas) {
        let n = node.n;
        let th = self.theme.clone();
        let ease = Timing(0.2, EASE);
        let size = match node.class {
            Class::Active => 80.0,
            Class::Grandchild => 15.0,
            _ => 40.0,
        };
        let layer = if node.hovered && node.clicked && node.class != Class::Grandchild { 0.95 } else { 1.0 };
        let layer = self.tw(now, n, "layer", layer, ease);
        let parent = pie.nodes[n].parent;
        let parent_hovered = parent.is_some_and(|p| pie.hovered == Some(p));
        let fill_c = if node.class == Class::Grandchild {
            let pa = node.parent_angle.or(parent.map(|p| pie.nodes[p].angle)).unwrap_or(0.0);
            hsl(pa, 0.5, if parent_hovered { 0.9 } else { 0.8 }, 1.0)
        } else if node.hovered && node.clicked {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            hsl(node.angle, 0.5, if node.hovered { 0.9 } else { 0.8 }, 1.0)
        };
        let icon = self.tw(now, n, "icon", if node.class == Class::Grandchild { 0.0 } else { 1.0 }, ease);
        let s = self.scale;
        let c = at * s;
        let r = size * layer * my * s / 2.0;
        let shadow_on = node.class != Class::Grandchild;
        let item = node.item.clone();
        let icon_color = th.c("icon-color");
        cv.add(c, r + 22.0 * s, move |pm, shift| {
            let c = c + shift;
            if shadow_on {
                shadow(pm, c, r, v(0.0, 0.0), 15.0 * s, [0.0, 0.0, 0.0, 0.4]);
            }
            disk(pm, c, r, fill_c);
            draw_icon(pm, &item, c, r * 1.8, icon_color, icon, true);
        });
    }

    fn purity_backdrop(&mut self, now: f32, screen: V) -> Option<Pixmap> {
        let th = self.theme.clone();
        let t = if self.animate { ((now - self.opened) / 3.5).clamp(0.0, 1.0) } else { 1.0 };
        let (a, b, c2, d) = VIGNETTE;
        let k = math::bezier(a, b, c2, d, t);
        let size = (screen.x.max(1.0) as u32, screen.y.max(1.0) as u32);
        if self.vignette.as_ref().map(|p| (p.width(), p.height())) != Some(size) {
            self.vignette = Pixmap::new(size.0, size.1).map(|mut pm| {
                let bg2 = th.c("background-secondary");
                let center = v(screen.x / 2.0, screen.y / 2.0);
                let rx = screen.x / 2.0 * 2f32.sqrt();
                let ry = screen.y / 2.0 * 2f32.sqrt();
                let stops = vec![
                    GradientStop::new(0.0, Color::TRANSPARENT),
                    GradientStop::new(0.4, Color::TRANSPARENT),
                    GradientStop::new(0.7, rgba(alpha(bg2, 0.35))),
                    GradientStop::new(0.88, rgba(alpha(bg2, 0.75))),
                    GradientStop::new(1.0, rgba(bg2)),
                ];
                let tr = Transform::from_row(1.0, 0.0, 0.0, ry / rx, 0.0, center.y - center.y * ry / rx);
                if let Some(shader) = RadialGradient::new(Point::from_xy(center.x, center.y), Point::from_xy(center.x, center.y), rx, stops, SpreadMode::Pad, tr) {
                    let mut paint = Paint::default();
                    paint.shader = shader;
                    if let Some(rect) = Rect::from_xywh(0.0, 0.0, screen.x, screen.y) {
                        pm.fill_rect(rect, &paint, Transform::identity(), None);
                    }
                }
                pm
            });
        }
        let main = th.c("background-main");
        let img = self.vignette.as_ref()?;
        let mut pm = self.spare.take().filter(|pm| pm.width() == size.0 && pm.height() == size.1).or_else(|| Pixmap::new(size.0, size.1))?;
        if main[3] <= 0.0 {
            pm.data_mut().copy_from_slice(img.data());
            if k < 1.0 {
                let opacity = (k * 255.0 + 0.5) as u32;
                for b in pm.data_mut() {
                    *b = ((*b as u32 * opacity + 127) / 255) as u8;
                }
            }
            return Some(pm);
        }
        pm.fill(rgba(main));
        let paint = PixmapPaint { opacity: k, ..Default::default() };
        pm.draw_pixmap(0, 0, img.as_ref(), &paint, Transform::identity(), None);
        Some(pm)
    }

    fn purity_layers(&mut self, pie: &Pie, node: &Node, at: V, now: f32, cv: &mut Canvas) {
        let n = node.n;
        let th = self.theme.clone();
        let s = self.scale;
        let c = at * s;
        match node.class {
            Class::Active => {
                let others_hovered = pie.hovered.is_some_and(|h| h != pie.center);
                let inner = self.tw(now, n, "inner", if others_hovered { 0.4 } else { 1.0 }, Timing(0.15, EASE_OUT));
                let plain = !pie.has_submenu_children();
                let ring = th.c("ring-color");
                let inner_c = th.c("inner-circle");
                let clicked = pie.clicked == Some(n);
                let parent = pie.nodes[n].parent;
                let parent_hovered = parent.is_some_and(|p| pie.hovered == Some(p));
                let parent_clicked = parent.is_some_and(|p| pie.clicked == Some(p));
                let back = pie.nodes[n].parent_wedge.filter(|_| n != 0);
                let back_timing = if parent_hovered { Timing(0.08, EASE_OUT) } else { Timing(1.0, SLOW_FADE) };
                let back_alpha = self.tw(now, n, "back", if parent_hovered { 1.0 } else { 0.0 }, back_timing);
                let icon_c = th.c("icon-color");
                cv.add(c, 160.0 * s, move |pm, shift| {
                    let c = c + shift;
                    if let Some(p) = annulus(c, 78.0 * s, 142.0 * s) {
                        if plain {
                            fill(pm, &p, mix([100.0 / 255.0, 100.0 / 255.0, 100.0 / 255.0, 0.45], ring, 0.35));
                            circle_line(pm, c, 141.25 * s, 1.5 * s, [1.0, 1.0, 1.0, 0.75]);
                            circle_line(pm, c, 143.0 * s, 3.0 * s, [1.0, 1.0, 1.0, 0.12]);
                        } else {
                            fill(pm, &p, ring);
                        }
                    }
                    if let Some(p) = annulus(c, 62.0 * s, 70.0 * s) {
                        fill(pm, &p, alpha(inner_c, inner));
                        if clicked {
                            fill(pm, &p, [215.0 / 255.0, 248.0 / 255.0, 1.0, 0.95]);
                        }
                    }
                    if let Some((ws, we)) = back {
                        if back_alpha > 0.01 {
                            purity_sector(pm, c, s, ws, we, [1.0, 1.0, 1.0, 1.0], back_alpha, parent_clicked);
                        }
                        let mid = (ws + we) / 2.0;
                        let apex = c + math::dir(mid, 118.0 * s);
                        let base = c + math::dir(mid, 106.0 * s);
                        let side = math::dir(mid + 90.0, 8.0 * s);
                        let mut pb = PathBuilder::new();
                        pb.move_to(apex.x, apex.y);
                        pb.line_to(base.x + side.x, base.y + side.y);
                        pb.line_to(base.x - side.x, base.y - side.y);
                        pb.close();
                        if let Some(p) = pb.finish() {
                            if parent_hovered {
                                let shape = p.clone();
                                let radius = if parent_clicked { 6.0 } else { 3.0 } * s;
                                glow(pm, apex, 30.0 * s, radius, [1.0, 1.0, 1.0, 0.8], v(0.0, 0.0), &move |tmp, mid| {
                                    let tr = Transform::from_translate(mid.x - apex.x, mid.y - apex.y);
                                    tmp.fill_path(&shape, &solid([1.0, 1.0, 1.0, 1.0]), FillRule::Winding, tr, None);
                                });
                                fill(pm, &p, [0.0, 0.0, 0.0, 1.0]);
                            } else {
                                fill(pm, &p, icon_c);
                            }
                        }
                    }
                });
            }
            Class::Child => {
                let hovered = node.hovered;
                let clicked = node.clicked;
                let (ws, we) = pie.nodes[n].wedge;
                let sector_timing = if hovered { Timing(0.08, EASE_OUT) } else { Timing(1.0, SLOW_FADE) };
                let sector_alpha = self.tw(now, n, "sector", if hovered || clicked { 1.0 } else { 0.0 }, sector_timing);
                let scale_target = if hovered && clicked { 0.92 } else if hovered { 1.1 } else { 1.0 };
                let icon_scale = self.tw(now, n, "icon-scale", scale_target, Timing(0.1, EASE));
                let color = if hovered { th.c("icon-hover-color") } else { th.c("icon-color") };
                let color = self.tw_color(now, n, ["r", "g", "b", "a"], color, Timing(0.1, EASE));
                let sector_color = if node.kind == Kind::Submenu { th.c("sector-color-submenu") } else { th.c("sector-color-action") };
                let center = pie.center_position() * s;
                let item = node.item.clone();
                let key_alpha = if self.quick_keys && !Self::child_hovered(pie) { ((now - self.opened - 0.8) / 0.3).clamp(0.0, 1.0) } else { 0.0 };
                let key = node.key.clone();
                let mid = (ws + we) / 2.0;
                if sector_alpha > 0.01 {
                    cv.add(center, 190.0 * s, move |pm, shift| purity_sector(pm, center + shift, s, ws, we, sector_color, sector_alpha, clicked));
                }
                cv.add(c, 40.0 * s, move |pm, shift| draw_icon(pm, &item, c + shift, 32.0 / 0.75 * s * icon_scale, color, 1.0, false));
                if key_alpha > 0.0 {
                    let kc = c + math::dir(mid, 65.0 * s);
                    cv.add(kc, 20.0 * s, move |pm, shift| {
                        let kc = kc + shift;
                        if let Some(f) = bold_font() {
                            text(pm, f, &key.to_uppercase(), kc + v(0.0, 1.0) * s, 24.0 * s, [0.0, 0.0, 0.0, 0.6 * key_alpha], None);
                            text(pm, f, &key.to_uppercase(), kc, 24.0 * s, [1.0, 1.0, 1.0, key_alpha], None);
                        }
                    });
                }
            }
            _ => {}
        }
    }

    fn selection_wedges(&mut self, pie: &Pie, c: V, cv: &mut Canvas) {
        let th = self.theme.clone();
        let s = self.scale;
        let hovered = pie.hovered.and_then(|h| {
            if h == pie.center {
                None
            } else if Some(h) == pie.nodes[pie.center].parent {
                pie.nodes[pie.center].parent_wedge
            } else {
                Some(pie.nodes[h].wedge)
            }
        });
        let area = v(self.area.0 as f32, self.area.1 as f32);
        let corners = [v(0.0, 0.0), v(area.x, 0.0), v(0.0, area.y), area];
        let far = corners.iter().map(|&p| math::dist(p, c)).fold(0.0, f32::max);
        let style = th.style;
        let reach = match style {
            Style::Default => far * 0.5,
            Style::NeonLights => 500.0 * s,
            _ => 380.0 * s,
        };
        cv.add(c, reach, move |pm, shift| {
            let c = c + shift;
            let t = |px: f32| px * s / reach;
            match style {
                Style::Default => {
                    let base = th.c("wedge-color");
                    let high = th.c("wedge-highlight-color");
                    let edge = (100.0 * s / reach).min(1.0);
                    let profile = |col: Rgba| vec![(0.0, col), (edge, col), (1.0, alpha(col, 0.0))];
                    match hovered {
                        Some((a0, a1)) => {
                            if let Some(p) = sector(c, 0.0, reach, a1, a0 + 360.0) {
                                radial(pm, c, reach, &profile(base), Some(&p));
                            }
                            if let Some(p) = sector(c, 0.0, reach, a0, a1) {
                                radial(pm, c, reach, &profile(high), Some(&p));
                            }
                        }
                        None => radial(pm, c, reach, &profile(base), None),
                    }
                }
                Style::NeonLights => {
                    if let Some((a0, a1)) = hovered {
                        let col = th.c("wedge-color");
                        let stops = vec![(0.0, alpha(col, 0.0)), (t(100.0), alpha(col, 0.0)), (t(150.0), col), (1.0, alpha(col, 0.0))];
                        if let Some(p) = sector(c, 0.0, reach, a0, a1) {
                            radial(pm, c, reach, &stops, Some(&p));
                        }
                    }
                }
                Style::Purity => {
                    let (h, sat, l) = theme::to_hsl(th.c("wedge-color"));
                    let base = hsl(h, sat, l * 0.4, 0.3);
                    let high = hsl(h, sat, l, 0.45);
                    match hovered {
                        Some((a0, a1)) => {
                            let profile = |col: Rgba| vec![(0.0, alpha(col, 0.0)), (t(70.0), alpha(col, 0.0)), (t(80.0), col), (1.0, alpha(col, 0.0))];
                            if let Some(p) = sector(c, 0.0, reach, a1, a0 + 360.0) {
                                radial(pm, c, reach, &profile(base), Some(&p));
                            }
                            if let Some(p) = sector(c, 0.0, reach, a0, a1) {
                                radial(pm, c, reach, &profile(high), Some(&p));
                            }
                        }
                        None => radial(pm, c, reach, &[(0.0, base), (t(40.0), base), (1.0, alpha(base, 0.0))], None),
                    }
                }
                _ => {}
            }
        });
    }

    fn separators(&mut self, pie: &Pie, c: V, cv: &mut Canvas) {
        let th = self.theme.clone();
        let s = self.scale;
        let mut angles: Vec<f32> = pie.nodes[pie.center].children.iter().map(|&ch| pie.nodes[ch].wedge.0).collect();
        if pie.center != 0 {
            if let Some((start, _)) = pie.nodes[pie.center].parent_wedge {
                angles.push(start);
            }
        }
        if angles.len() < 2 {
            return;
        }
        let col = th.c("separator-color");
        cv.add(c, 300.0 * s, move |pm, shift| {
            let c = c + shift;
            let stroke = Stroke { width: s.max(1.0), ..Default::default() };
            for a in &angles {
                let d = math::dir(*a, 1.0);
                let steps = 50;
                for i in 0..steps {
                    let t0 = 50.0 + 250.0 * i as f32 / steps as f32;
                    let t1 = 50.0 + 250.0 * (i + 1) as f32 / steps as f32;
                    let mid = (t0 + t1) / 2.0;
                    let k = if mid < 100.0 { (mid - 50.0) / 50.0 } else { 1.0 - (mid - 100.0) / 200.0 };
                    let p0 = c + d * (t0 * s);
                    let p1 = c + d * (t1 * s);
                    let mut pb = PathBuilder::new();
                    pb.move_to(p0.x, p0.y);
                    pb.line_to(p1.x, p1.y);
                    if let Some(p) = pb.finish() {
                        pm.stroke_path(&p, &solid(alpha(col, k.clamp(0.0, 1.0))), &stroke, Transform::identity(), None);
                    }
                }
            }
        });
    }
}

fn closest(angle: f32, to: f32) -> f32 {
    math::closest_equivalent(angle, to)
}

fn purity_sector(pm: &mut Pixmap, c: V, s: f32, a0: f32, a1: f32, color: Rgba, opacity: f32, clicked: bool) {
    let (a0, a1) = (a0 + 0.5, a1 - 0.5);
    if a1 <= a0 {
        return;
    }
    let bright = [215.0 / 255.0, 248.0 / 255.0, 1.0, 0.95];
    let boost = if clicked { 1.45 } else { 1.0 };
    let tint = |c: Rgba, k: f32| [(c[0] * boost).min(1.0), (c[1] * boost).min(1.0), (c[2] * boost).min(1.0), c[3] * k * opacity];
    if let Some(p) = sector(c, 62.0 * s, 70.0 * s, a0, a1) {
        fill(pm, &p, tint(bright, 1.0));
    }
    if let Some(p) = sector(c, 78.0 * s, 142.0 * s, a0, a1) {
        let r = 142.0 * s;
        let t = |px: f32| px * s / r;
        let stops = [
            (0.0, tint(color, 0.0)),
            (t(77.9), tint(color, 0.0)),
            (t(78.0), tint(color, 1.0)),
            (t(88.0), tint(color, 0.85)),
            (t(98.0), tint(color, 0.64)),
            (t(122.0), tint(color, 0.36)),
            (t(138.0), tint(color, 0.16)),
            (1.0, tint(color, 0.0)),
        ];
        radial(pm, c, r, &stops, Some(&p));
    }
    if clicked {
        if let Some(p) = sector(c, 146.0 * s, 150.0 * s, a0, a1) {
            fill(pm, &p, tint(bright, 1.0));
        }
    }
}

fn neon_ring(pm: &mut Pixmap, c: V, size: f32, spin: f32, glow_color: Rgba, colored: bool, opacity: f32) {
    let far = size * std::f32::consts::FRAC_1_SQRT_2;
    let grey = [120.0 / 255.0, 120.0 / 255.0, 120.0 / 255.0, 1.0];
    let (core, halo) = if colored { ([1.0, 1.0, 1.0, 1.0], glow_color) } else { (grey, grey) };
    let k = |a: f32| alpha(halo, a * opacity);
    let stops = [
        (0.20, k(0.0)),
        (0.27, k(0.1)),
        (0.30, k(0.2)),
        (0.33, k(0.6)),
        (0.34, alpha(core, opacity)),
        (0.36, alpha(core, opacity)),
        (0.37, k(0.6)),
        (0.40, k(0.2)),
        (0.47, k(0.1)),
        (0.60, k(0.0)),
    ];
    let gap = 4.88;
    for half in [0.0f32, 180.0] {
        let a0 = spin + half + gap;
        let a1 = spin + half + 180.0 - gap;
        if let Some(p) = sector(c, 0.0, far * 0.6, a0, a1) {
            radial(pm, c, far, &stops, Some(&p));
        }
    }
}
