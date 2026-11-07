
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct V {
    pub x: f32,
    pub y: f32,
}

pub fn v(x: f32, y: f32) -> V {
    V { x, y }
}

impl std::ops::Add for V {
    type Output = V;
    fn add(self, o: V) -> V {
        v(self.x + o.x, self.y + o.y)
    }
}

impl std::ops::Sub for V {
    type Output = V;
    fn sub(self, o: V) -> V {
        v(self.x - o.x, self.y - o.y)
    }
}

impl std::ops::Mul<f32> for V {
    type Output = V;
    fn mul(self, k: f32) -> V {
        v(self.x * k, self.y * k)
    }
}

pub fn len(a: V) -> f32 {
    (a.x * a.x + a.y * a.y).sqrt()
}

pub fn dist(a: V, b: V) -> f32 {
    len(a - b)
}

pub fn dot(a: V, b: V) -> f32 {
    a.x * b.x + a.y * b.y
}

pub fn clockwise(a: V, b: V) -> bool {
    a.x * b.y - a.y * b.x < 0.0
}

pub fn dist_to_segment(p: V, a: V, b: V) -> f32 {
    let l = b - a;
    let ll = len(l);
    let d = l * (1.0 / ll);
    let proj = dot(p - a, d);
    if proj < 0.0 {
        return dist(p, a);
    }
    if proj > ll {
        return dist(p, b);
    }
    dist(p, a + d * proj)
}

pub fn angle_diff(a: f32, b: f32) -> f32 {
    let d = ((a % 360.0) - (b % 360.0)).abs();
    d.min(360.0 - d)
}

pub fn closest_equivalent(mut angle: f32, to: f32) -> f32 {
    if (to - angle).abs() > 180.0 {
        angle += ((to - angle) / 360.0).round() * 360.0;
    }
    angle
}

fn smaller_than(angle: f32, than: f32) -> f32 {
    angle + ((than - angle) / 360.0).floor() * 360.0
}

fn larger_than(angle: f32, than: f32) -> f32 {
    angle + ((than - angle) / 360.0).ceil() * 360.0
}

pub fn between(a: f32, start: f32, end: f32) -> bool {
    (a > start && a <= end) || (a - 360.0 > start && a - 360.0 <= end) || (a + 360.0 > start && a + 360.0 <= end)
}

fn consecutive(start: f32, center: f32, end: f32) -> (f32, f32, f32) {
    let center = center % 360.0;
    (smaller_than(start, center), center, larger_than(end, center))
}

pub fn angle_of(a: V) -> f32 {
    let r = (a.y.atan2(a.x).to_degrees() + 90.0) % 360.0;
    if r < 0.0 { 360.0 + r } else { r }
}

pub fn dir(angle: f32, length: f32) -> V {
    let r = (angle - 90.0).to_radians();
    v(r.cos() * length, r.sin() * length)
}

pub fn fix_fixed_angles(angles: &mut [Option<f32>]) {
    let mut first = None;
    let mut last: Option<f32> = None;
    for a in angles.iter_mut().flatten() {
        *a = match last {
            None => larger_than(*a, 0.0),
            Some(l) => larger_than(*a, l),
        };
        if first.is_none() {
            first = Some(*a);
        }
        last = Some(*a);
    }
    let Some(first) = first else { return };

    let mut prev: Option<f32> = None;
    for a in angles.iter_mut() {
        if let Some(x) = *a {
            if prev == Some(x) {
                *a = None;
            } else {
                prev = Some(x);
            }
        }
    }
    for a in angles.iter_mut() {
        if a.is_some_and(|x| x >= first + 360.0) {
            *a = None;
        }
    }
}

pub fn item_angles(fixed_in: &[Option<f32>], parent: Option<f32>) -> Vec<f32> {
    let n = fixed_in.len();
    let mut out = vec![0.0; n];
    if n == 0 {
        return out;
    }

    let mut fixed: Vec<(f32, usize)> = vec![];
    for (i, a) in fixed_in.iter().enumerate() {
        if let Some(a) = a {
            if *a >= 0.0 {
                fixed.push((*a, i));
            }
        }
    }
    if let Some(p) = parent {
        for f in fixed.iter_mut() {
            if (f.0 - p).abs() < 0.0001 {
                f.0 += 0.1;
            }
        }
    }
    let mut i = 0;
    while i + 1 < fixed.len() {
        if fixed[i].0 > fixed[i + 1].0 {
            fixed.remove(i + 1);
        } else {
            i += 1;
        }
    }

    if fixed.is_empty() {
        let mut first = 0.0;
        if let Some(p) = parent {
            let wedge = 360.0 / (n + 1) as f32;
            let mut min = 360.0f32;
            for i in 0..n {
                min = min.min((p + (i + 1) as f32 * wedge) % 360.0);
            }
            first = min;
        }
        fixed.push((first, 0));
        out[0] = first;
    }

    let mut parent = parent;
    for i in 0..fixed.len() {
        let (begin_angle, begin) = fixed[i];
        let (mut end_angle, end) = fixed[(i + 1) % fixed.len()];
        out[begin] = begin_angle;
        if end_angle <= begin_angle {
            end_angle += 360.0;
        }
        let mut count_in = (end + n - begin - 1) % n;
        let mut parent_in = false;
        if let Some(p) = parent.as_mut() {
            if *p < begin_angle {
                *p += 360.0;
            }
            parent_in = *p > begin_angle && *p < end_angle;
            if parent_in {
                count_in += 1;
            }
        }
        let gap = (end_angle - begin_angle) / (count_in + 1) as f32;
        let mut index = (begin + 1) % n;
        let mut count = 1;
        let mut need_gap = parent_in;
        while index != end {
            let mut a = begin_angle + gap * count as f32;
            if need_gap && a + gap / 2.0 - parent.unwrap() > 0.0 {
                count += 1;
                a = begin_angle + gap * count as f32;
                need_gap = false;
            }
            out[index] = a % 360.0;
            index = (index + 1) % n;
            count += 1;
        }
    }
    out
}

fn crop(start: f32, center: f32, end: f32, c: f32) -> (f32, f32) {
    let mut s = start;
    let mut e = end;
    if between(c, start, center) {
        s = c;
    }
    if between(c, center, end) {
        e = c;
    }
    (s, e)
}

fn scale_wedge(start: f32, center: f32, end: f32, k: f32) -> (f32, f32) {
    (center - (center - start) * k, center + (end - center) * k)
}

pub type Wedge = (f32, f32);

pub fn item_wedges(angles: &[f32], parent: Option<f32>) -> (Vec<Wedge>, Option<Wedge>) {
    let n = angles.len();
    match (n, parent) {
        (0, None) => return (vec![], None),
        (0, Some(p)) => return (vec![], Some((p - 180.0, p + 180.0))),
        (1, None) => return (vec![(0.0, 360.0)], None),
        (1, Some(p)) => {
            let (s, c, e) = consecutive(p, angles[0], p + 360.0);
            let (s, e) = scale_wedge(s, c, e, 0.5);
            return (vec![(s, e)], Some((e, s + 360.0)));
        }
        _ => {}
    }

    let mut wedges = vec![];
    let mut ps = None;
    let mut pe = None;
    for i in 0..n {
        let (mut s, mut c, mut e) = consecutive(angles[(i + n - 1) % n], angles[i], angles[(i + 1) % n]);
        if let Some(p) = parent {
            if between(p, s, c) {
                ps = Some(s);
            } else if between(p, c, e) {
                pe = Some(e);
            }
            let (s2, e2) = crop(s, c, e, p);
            (s, c, e) = consecutive(s2, c, e2);
        }
        let (s, e) = scale_wedge(s, c, e, 0.5);
        wedges.push((s, e));
    }
    if let (Some(p), Some(s), Some(e)) = (parent, ps, pe) {
        let (s, c, e) = consecutive(s, p, e);
        let (s, e) = scale_wedge(s, c, e, 0.5);
        return (wedges, Some((s, e)));
    }
    (wedges, None)
}

pub fn clamp_to_monitor(p: V, radius: f32, size: V) -> V {
    let x = p.x.clamp(radius, (size.x - radius).max(radius));
    let y = p.y.clamp(radius, (size.y - radius).max(radius));
    v(x.floor(), y.floor())
}

pub fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let f = |t: f32, a: f32, b: f32| 3.0 * (1.0 - t) * (1.0 - t) * t * a + 3.0 * (1.0 - t) * t * t * b + t * t * t;
    let mut lo = 0.0;
    let mut hi = 1.0;
    let mut t = x;
    for _ in 0..30 {
        let cx = f(t, x1, x2);
        if (cx - x).abs() < 1e-5 {
            break;
        }
        if cx < x { lo = t } else { hi = t }
        t = (lo + hi) / 2.0;
    }
    f(t, y1, y2)
}
