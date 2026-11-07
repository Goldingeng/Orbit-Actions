use std::time::{Duration, Instant};

use crate::math::*;
use crate::menu::{Item, Kind, Settings};

#[derive(Clone, Copy, PartialEq)]
pub enum Button {
    Released,
    Clicked,
    Dragged,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Pick {
    Active,
    SubmenuOnly,
    Parent,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Class {
    Active,
    Parent,
    Child,
    Grandchild,
    Hidden,
}

pub enum Event {
    Open(Vec<usize>),
    Activate(Vec<usize>),
    Hover(Vec<usize>),
    Select(Vec<usize>),
    Sound(&'static str),
    Hide,
    Warp(V),
}

#[derive(Clone, Copy)]
pub struct Input {
    pub button: Button,
    pub abs: V,
    pub rel: V,
    pub distance: f32,
    pub angle: f32,
}

pub struct Node {
    pub item: Item,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    pub path: Vec<usize>,
    pub angle: f32,
    pub wedge: Wedge,
    pub parent_wedge: Option<Wedge>,
    pub pos: Option<V>,
    pub key: String,
    pub connector: (f32, f32),
    pub last_connector: f32,
}

pub struct Gesture {
    start: Option<V>,
    end: Option<V>,
    pub deadline: Option<Instant>,
    pending: V,
    pub min_len: f32,
    pub min_angle: f32,
    pub jitter: f32,
    pub pause: u32,
    pub dead_zone: f32,
    pub fixed_len: f32,
}

impl Gesture {
    pub fn reset(&mut self, corner: Option<V>) {
        self.deadline = None;
        self.start = corner;
        self.end = corner;
    }

    fn motion(&mut self, p: V) -> Option<V> {
        let (Some(start), Some(end)) = (self.start, self.end) else {
            self.start = Some(p);
            self.end = Some(p);
            return None;
        };
        let stroke = end - start;
        let stroke_len = len(stroke);

        if self.fixed_len > 0.0 {
            let min = self.fixed_len + self.dead_zone;
            self.end = Some(p);
            if stroke_len > min {
                let ideal = start + stroke * (min / stroke_len);
                self.reset(Some(ideal));
                return Some(ideal);
            }
            return None;
        }

        if stroke_len > self.min_len {
            let tip = p - end;
            let tip_len = len(tip);
            if tip_len > self.jitter {
                self.deadline = None;
                let cos = (tip.x / tip_len) * stroke.x / stroke_len + (tip.y / tip_len) * stroke.y / stroke_len;
                if cos.clamp(-1.0, 1.0).acos().to_degrees() > self.min_angle {
                    self.reset(Some(end));
                    return Some(end);
                }
                self.end = Some(p);
            }
            if self.deadline.is_none() {
                self.deadline = Some(Instant::now() + Duration::from_millis(self.pause as u64));
                self.pending = p;
            }
        } else {
            self.end = Some(p);
        }
        None
    }
}

pub struct Pie {
    pub nodes: Vec<Node>,
    pub center: usize,
    pub hovered: Option<usize>,
    pub clicked: Option<usize>,
    pub dragged: Option<usize>,
    pub input: Input,
    pub text: Option<usize>,
    pub events: Vec<Event>,
    pub hidden: bool,
    pub size: V,
    pub max_radius: f32,
    mouse: V,
    fixed: Option<(f32, f32)>,
    anchored: bool,

    dead_zone: f32,
    parent_distance: f32,
    max_selection: f32,
    warp: bool,
    rmb_parent: bool,

    drag_threshold: f32,
    marking: bool,
    turbo: bool,
    hover_mode: bool,
    center_on_release: bool,
    position: V,
    button: Button,
    deferred: bool,
    click: Option<V>,
    keydown: V,
    pointer_center: V,
    radius: f32,
    ignore: u32,
    pub gesture: Gesture,
}

impl Pie {
    pub fn new(root: &Item, s: &Settings, size: V, mouse: V, fixed: Option<(f32, f32)>, anchored: bool, hover_mode: bool) -> Pie {
        let mut pie = Pie {
            nodes: vec![],
            center: usize::MAX,
            hovered: None,
            clicked: None,
            dragged: None,
            input: Input { button: Button::Released, abs: mouse, rel: v(0.0, 0.0), distance: 0.0, angle: 0.0 },
            text: None,
            events: vec![],
            hidden: false,
            size,
            max_radius: s.theme.max_radius,
            mouse,
            fixed,
            anchored,
            dead_zone: s.dead_zone,
            parent_distance: s.parent_distance,
            max_selection: s.max_radius,
            warp: s.warp,
            rmb_parent: s.rmb_parent,
            drag_threshold: s.drag_threshold,
            marking: s.marking && !anchored,
            turbo: s.turbo && !anchored,
            hover_mode,
            center_on_release: s.center_on_release,
            position: v(0.0, 0.0),
            button: Button::Released,
            deferred: !s.warp && fixed.is_some(),
            click: Some(v(0.0, 0.0)),
            keydown: v(0.0, 0.0),
            pointer_center: v(0.0, 0.0),
            radius: 0.0,
            ignore: 2,
            gesture: Gesture {
                start: None,
                end: None,
                deadline: None,
                pending: v(0.0, 0.0),
                min_len: s.stroke_len,
                min_angle: s.stroke_angle,
                jitter: s.jitter,
                pause: s.pause,
                dead_zone: s.dead_zone,
                fixed_len: s.fixed_stroke,
            },
        };
        pie.build(root);
        pie.events.push(Event::Sound("openMenu"));

        let start = pie.initial_position();
        let to_pointer = mouse - start;
        pie.input = Input { button: Button::Released, abs: mouse, rel: to_pointer, distance: len(to_pointer), angle: angle_of(to_pointer) };
        pie.open_submenu(0, Some(start));
        if pie.warp && fixed.is_some() {
            pie.events.retain(|event| !matches!(event, Event::Warp(_)));
            pie.events.push(Event::Warp(pie.center_position()));
        }
        pie
    }

    fn build(&mut self, root: &Item) {
        let mut queue = std::collections::VecDeque::new();
        queue.push_back((root.clone(), None::<usize>, 0usize, 0.0f32, (0.0f32, 0.0f32)));

        while let Some((item, parent, index, angle, wedge)) = queue.pop_front() {
            let id = self.nodes.len();
            let path = match parent {
                Some(p) => {
                    let mut path = self.nodes[p].path.clone();
                    path.push(index);
                    path
                }
                None => vec![],
            };
            let key = item.key.clone().unwrap_or_else(|| (index + 1).to_string());

            let mut children = if item.kind == Kind::Button { vec![] } else { item.children.clone() };
            let mut angles = vec![];
            let mut parent_wedge = None;
            let mut wedges = vec![];
            if item.kind != Kind::Button {
                let fixed: Vec<Option<f32>> = children.iter().map(|c| c.angle).collect();
                if item.kind == Kind::Root {
                    angles = item_angles(&fixed, None);
                    wedges = item_wedges(&angles, None).0;
                } else {
                    let back = (angle + 180.0) % 360.0;
                    angles = item_angles(&fixed, Some(back));
                    let (w, pw) = item_wedges(&angles, Some(back));
                    wedges = w;
                    parent_wedge = pw;
                }
            }

            if let Some(p) = parent {
                self.nodes[p].children.push(id);
            }
            let mut node_item = item;
            node_item.children = vec![];
            self.nodes.push(Node {
                item: node_item,
                children: vec![],
                parent,
                path,
                angle,
                wedge,
                parent_wedge,
                pos: Some(v(0.0, 0.0)),
                key,
                connector: (0.0, 0.0),
                last_connector: 0.0,
            });
            for (i, child) in children.drain(..).enumerate() {
                queue.push_back((child, Some(id), i, angles[i], wedges[i]));
            }
        }
    }

    pub fn kind(&self, n: usize) -> Kind {
        self.nodes[n].item.kind
    }

    pub fn class(&self, n: usize) -> Class {
        if n == self.center {
            return Class::Active;
        }
        if self.is_ancestor(n) {
            return Class::Parent;
        }
        let Some(p) = self.nodes[n].parent else { return Class::Hidden };
        if p == self.center {
            return Class::Child;
        }
        match self.class(p) {
            Class::Child if self.kind(p) == Kind::Submenu => Class::Grandchild,
            Class::Parent => Class::Grandchild,
            _ => Class::Hidden,
        }
    }

    fn is_ancestor(&self, n: usize) -> bool {
        let mut cur = self.nodes[self.center].parent;
        while let Some(c) = cur {
            if c == n {
                return true;
            }
            cur = self.nodes[c].parent;
        }
        false
    }

    pub fn center_position(&self) -> V {
        let mut sum = v(0.0, 0.0);
        let mut cur = Some(if self.center == usize::MAX { 0 } else { self.center });
        while let Some(c) = cur {
            sum = sum + self.nodes[c].pos.unwrap_or_default();
            cur = self.nodes[c].parent;
        }
        sum
    }

    fn initial_position(&self) -> V {
        match self.fixed {
            Some((x, y)) => v(self.size.x * x, self.size.y * y),
            None => self.mouse,
        }
    }

    fn parent_of_center(&self) -> Option<usize> {
        self.nodes[self.center].parent
    }

    pub fn hide(&mut self) {
        if !self.hidden {
            self.hidden = true;
            self.events.push(Event::Sound("closeMenu"));
            self.events.push(Event::Hide);
        }
    }

    fn open_submenu(&mut self, n: usize, coords: Option<V>) {
        if self.center == n {
            return;
        }
        self.clicked = None;
        self.dragged = None;

        let picked_parent = self.center != usize::MAX && Some(n) == self.parent_of_center();
        if self.center != usize::MAX {
            self.events.push(Event::Sound(if picked_parent { "closeSubmenu" } else { "openSubmenu" }));
        }
        self.center = n;

        if self.kind(n) == Kind::Root {
            self.nodes[n].pos = Some(if self.anchored { self.initial_position() } else { coords.unwrap_or(self.input.abs) });
        } else {
            let mut distance = self.parent_distance;
            if !self.anchored {
                distance = if picked_parent {
                    len(self.nodes[n].pos.unwrap_or_default())
                } else {
                    self.parent_distance.max(self.input.distance)
                };
            }
            self.nodes[n].pos = Some(dir(self.nodes[n].angle, distance));
            let target = if self.anchored { self.initial_position() } else { coords.unwrap_or(self.input.abs) };
            let offset = target - self.center_position();
            self.nodes[0].pos = Some(self.nodes[0].pos.unwrap_or_default() + offset);
        }

        let position = self.center_position();
        let clamped = clamp_to_monitor(position, self.max_radius, self.size);
        let offset = v((clamped.x - position.x).trunc(), (clamped.y - position.y).trunc());
        if offset.x != 0.0 || offset.y != 0.0 {
            if !self.anchored && self.warp {
                self.events.push(Event::Warp(clamped));
            }
            self.nodes[0].pos = Some(self.nodes[0].pos.unwrap_or_default() + offset);
        }

        self.input.abs = clamped;
        self.input.rel = v(0.0, 0.0);
        self.input.distance = 0.0;
        self.input.angle = 0.0;

        self.update(clamped, clamped, self.button);
        self.gesture.reset(None);
        self.gesture.motion(clamped);
        self.keydown = clamped;
        self.radius = self.dead_zone;

        self.redraw();
    }

    fn select_parent(&mut self, coords: Option<V>) {
        if self.center == 0 {
            self.hide();
            return;
        }
        if let Some(p) = self.parent_of_center() {
            self.open_submenu(p, coords);
        }
    }

    fn hover_item(&mut self, item: Option<usize>) {
        if self.hovered == item {
            return;
        }
        self.hovered = item;
        if let Some(n) = item {
            if n == self.center {
                self.events.push(Event::Sound("hoverCenter"));
            } else if Some(n) == self.parent_of_center() {
                self.events.push(Event::Sound("hoverParent"));
            } else if self.kind(n) != Kind::Root {
                let sound = if self.kind(n) == Kind::Button { "hoverButton" } else { "hoverSubmenu" };
                self.events.push(Event::Sound(sound));
                self.events.push(Event::Hover(self.nodes[n].path.clone()));
            }
        }
    }

    fn hover_angle(&mut self, angle: f32) {
        let rel = dir(angle, self.parent_distance);
        self.input = Input { button: self.input.button, abs: self.center_position() + rel, rel, distance: self.parent_distance, angle };
        self.redraw();
    }

    fn drag_item(&mut self, item: Option<usize>) {
        self.clicked = None;
        self.dragged = item;
    }

    pub fn redraw(&mut self) {
        let next = self.compute_hovered();
        match next {
            None => self.text = None,
            Some(n) if n == self.center || Some(n) == self.parent_of_center() => self.text = None,
            Some(n) if self.kind(n) != Kind::Root && Some(n) != self.hovered => self.text = Some(n),
            _ => {}
        }
        if next != self.hovered {
            self.hover_item(next);
        }
        if self.dragged.is_some() && self.dragged != self.hovered {
            self.drag_item(self.hovered);
        }
        if self.input.button == Button::Clicked && self.clicked.is_none() {
            self.clicked = self.hovered;
        }
        if self.input.button == Button::Dragged && self.dragged.is_none() && self.input.distance > self.dead_zone && self.hovered.is_some() {
            self.drag_item(self.hovered);
        }
        if self.input.button == Button::Dragged && self.dragged.is_some() && self.input.distance < self.dead_zone {
            self.drag_item(None);
        }
        if self.input.button == Button::Released {
            self.dragged = None;
            self.clicked = None;
        }

        let children = self.nodes[self.center].children.clone();
        for c in children {
            self.nodes[c].pos = if Some(c) == self.dragged { Some(self.input.rel) } else { None };
        }
        self.update_connectors();
    }

    fn compute_hovered(&self) -> Option<usize> {
        if self.input.distance < self.dead_zone {
            return Some(self.center);
        }
        if self.input.button != Button::Dragged && self.max_selection > 0.0 {
            let mut min = self.input.distance;
            if let Some(p) = self.parent_of_center() {
                let end = self.center_position();
                let start = end - self.nodes[p].pos.unwrap_or_default();
                min = min.min(dist_to_segment(self.input.abs, start, end));
            }
            if min > self.max_selection {
                return None;
            }
        }
        if self.kind(self.center) != Kind::Button {
            for &c in &self.nodes[self.center].children {
                let (s, e) = self.nodes[c].wedge;
                if between(self.input.angle, s, e) {
                    return Some(c);
                }
            }
        }
        self.parent_of_center()
    }

    fn update_connectors(&mut self) {
        let mut start = Some(self.center);
        let mut end = None;
        let mut draw = true;

        if self.kind(self.center) == Kind::Button {
            end = Some(self.center);
            start = self.parent_of_center();
        } else if self.dragged.is_some() && self.dragged.and_then(|d| self.nodes[d].parent) == Some(self.center) {
            end = self.dragged;
        } else if self.clicked.is_some() && self.clicked != Some(self.center) && self.clicked != self.parent_of_center() {
            end = self.clicked;
        } else if self.hovered.is_some() && self.hovered != Some(self.center) {
            end = self.hovered;
            draw = false;
        }

        while let Some(s) = start {
            if let Some(e) = end {
                let mut length = 0.0;
                let mut angle = self.nodes[e].angle;
                if let Some(p) = self.nodes[e].pos {
                    length = if draw { len(p) } else { 0.0 };
                    angle = angle_of(p);
                }
                angle = closest_equivalent(angle, self.nodes[s].last_connector);
                self.nodes[s].last_connector = angle;
                self.nodes[s].connector = (length, angle);
            } else {
                self.nodes[s].connector.0 = 0.0;
            }
            end = Some(s);
            start = self.nodes[s].parent;
            draw = true;
        }
    }

    fn selection(&mut self, coords: V, pick: Pick) {
        if self.hidden {
            return;
        }
        if pick == Pick::Parent {
            self.select_parent(Some(coords));
            return;
        }
        let item = self.hovered.or(self.clicked).or(self.dragged);

        if let Some(n) = item {
            if pick == Pick::SubmenuOnly && self.kind(n) != Kind::Button && self.input.distance > self.dead_zone {
                if Some(n) != self.parent_of_center() {
                    self.events.push(Event::Open(self.nodes[n].path.clone()));
                }
                self.open_submenu(n, Some(coords));
                return;
            }
            if pick == Pick::Active {
                if n == self.center {
                    self.events.push(Event::Activate(self.nodes[n].path.clone()));
                } else if self.kind(n) == Kind::Button {
                    self.events.push(Event::Select(self.nodes[n].path.clone()));
                } else if Some(n) != self.parent_of_center() {
                    self.events.push(Event::Open(self.nodes[n].path.clone()));
                }
                if self.kind(n) != Kind::Button {
                    self.open_submenu(n, Some(coords));
                }
                return;
            }
        }
        if pick == Pick::Active && item.is_none() {
            self.hide();
        }
    }

    pub fn center_path(&self) -> Vec<usize> {
        self.nodes[self.center].path.clone()
    }

    pub fn chain(&self, path: &[usize]) -> Vec<V> {
        let mut n = 0;
        let mut out = vec![self.nodes[0].pos.unwrap_or_default()];
        for &i in path {
            n = self.nodes[n].children[i];
            out.push(self.nodes[n].pos.unwrap_or_default());
        }
        out
    }

    pub fn restore(&mut self, path: &[usize], chain: &[V]) {
        self.events.clear();
        let mut n = 0;
        self.nodes[0].pos = Some(chain[0]);
        for (depth, &i) in path.iter().enumerate() {
            let Some(&next) = self.nodes[n].children.get(i) else { break };
            n = next;
            self.nodes[n].pos = Some(chain[depth + 1]);
        }
        self.center = n;
        self.ignore = 0;
        let at = self.center_position();
        self.input = Input { button: self.button, abs: at, rel: v(0.0, 0.0), distance: 0.0, angle: 0.0 };
        self.update(at, at, self.button);
        self.gesture.reset(None);
        self.keydown = at;
        self.radius = self.dead_zone;
        self.redraw();
    }

    pub fn close_submenu(&mut self) {
        if self.kind(self.center) == Kind::Submenu {
            self.select_parent(None);
        }
    }

    fn update(&mut self, pointer: V, center: V, button: Button) {
        if self.pointer_center == center && self.button == button && self.position == pointer {
            return;
        }
        self.pointer_center = center;
        self.button = button;
        self.position = pointer;
        let rel = pointer - center;
        self.input = Input { button, abs: pointer, rel, distance: len(rel), angle: angle_of(rel) };
        if self.center != usize::MAX {
            self.redraw();
        }
    }

    pub fn motion(&mut self, p: V, modifier: bool) {
        if self.ignore > 0 {
            self.ignore -= 1;
            return;
        }
        let in_click_zone = self.click.is_some_and(|c| dist(self.position, c) < self.drag_threshold);
        let mut next = self.button;
        if self.button == Button::Clicked && !in_click_zone {
            next = if self.marking { Button::Dragged } else { Button::Released };
        }
        let can_turbo = (self.turbo || self.hover_mode) && !self.deferred && self.button != Button::Dragged;
        if can_turbo && (self.hover_mode || modifier) && dist(self.position, self.keydown) > self.drag_threshold {
            next = Button::Dragged;
        }
        self.update(p, self.pointer_center, next);

        if self.button == Button::Dragged {
            if let Some(at) = self.gesture.motion(self.position) {
                self.gesture_pick(at);
            }
        }
    }

    fn gesture_pick(&mut self, at: V) {
        let pick = if self.hover_mode { Pick::Active } else { Pick::SubmenuOnly };
        self.selection(at, pick);
    }

    pub fn gesture_timeout(&mut self) {
        if self.gesture.deadline.is_some_and(|d| Instant::now() >= d) {
            let at = self.gesture.pending;
            self.gesture.reset(Some(at));
            self.gesture_pick(at);
        }
    }

    pub fn right_click(&mut self) {
        if self.rmb_parent {
            self.select_parent(None);
        } else {
            self.hide();
        }
    }

    pub fn back_button(&mut self) {
        let at = self.position;
        self.selection(at, Pick::Parent);
    }

    pub fn press(&mut self, p: V) {
        self.click = Some(p);
        self.update(p, self.pointer_center, Button::Clicked);
        self.gesture.reset(None);
    }

    pub fn release(&mut self, p: V) {
        self.gesture.reset(None);
        let by_click = self.button == Button::Clicked;
        let by_marking = self.button == Button::Dragged && dist(self.position, self.pointer_center) > self.radius;
        if by_click || by_marking {
            let at = self.position;
            self.selection(at, Pick::Active);
        }
        self.click = None;
        self.update(p, self.pointer_center, Button::Released);
    }

    pub fn modifier_down(&mut self) {
        if !self.deferred {
            self.keydown = self.position;
        }
    }

    pub fn modifier_up(&mut self, still_held: bool) {
        if !self.turbo || still_held {
            return;
        }
        self.deferred = false;
        let trigger = self.center_on_release
            || (self.button == Button::Dragged && dist(self.position, self.pointer_center) > self.radius);
        if trigger {
            self.gesture.reset(None);
            let at = self.position;
            self.selection(at, Pick::Active);
        }
        let at = self.position;
        let c = self.pointer_center;
        self.update(at, c, Button::Released);
    }

    pub fn key(&mut self, name: &str, down: bool) -> bool {
        if self.hidden {
            return false;
        }
        let center = self.center;
        if self.kind(center) != Kind::Button {
            if let Some(k) = self.nodes[center].item.activate_key.clone() {
                if k.to_lowercase() == name {
                    if down {
                        self.hover_item(Some(center));
                    } else {
                        self.events.push(Event::Activate(self.nodes[center].path.clone()));
                    }
                    return true;
                }
            }

            let children = self.nodes[center].children.clone();
            for (i, &c) in children.iter().enumerate() {
                let mut keys = vec![];
                match &self.nodes[c].item.key {
                    Some(k) => keys.push(k.to_lowercase()),
                    None if i < 9 => {
                        keys.push(format!("{}", i + 1));
                        keys.push(format!("num{}", i + 1));
                    }
                    None => {}
                }
                if keys.iter().any(|k| k == name) {
                    if down {
                        self.input.button = Button::Clicked;
                        self.hover_angle(self.nodes[c].angle);
                    } else {
                        if self.kind(c) == Kind::Submenu {
                            self.events.push(Event::Open(self.nodes[c].path.clone()));
                            let at = self.center_position();
                            self.open_submenu(c, Some(at));
                        } else {
                            self.events.push(Event::Select(self.nodes[c].path.clone()));
                        }
                        self.input.button = Button::Released;
                        self.redraw();
                    }
                    return true;
                }
            }
        }

        if down {
            let arrow = match name {
                "arrowleft" => Some(v(-1.0, 0.0)),
                "arrowup" => Some(v(0.0, -1.0)),
                "arrowright" => Some(v(1.0, 0.0)),
                "arrowdown" => Some(v(0.0, 1.0)),
                _ => None,
            };
            if let Some(d) = arrow {
                self.arrow(d);
                return true;
            }
        }

        if name == "enter" {
            if down {
                self.input.button = Button::Clicked;
                self.redraw();
            } else if let Some(h) = self.hovered {
                if h == self.center {
                    self.events.push(Event::Activate(self.nodes[h].path.clone()));
                } else if Some(h) == self.parent_of_center() {
                    self.select_parent(None);
                } else if self.kind(h) == Kind::Submenu {
                    self.events.push(Event::Open(self.nodes[h].path.clone()));
                    self.open_submenu(h, None);
                } else {
                    self.events.push(Event::Select(self.nodes[h].path.clone()));
                }
                self.input.button = Button::Released;
                self.redraw();
            }
            return true;
        }
        false
    }

    fn arrow(&mut self, d: V) {
        let mut candidates: Vec<(usize, f32, f32)> = vec![];
        if self.kind(self.center) != Kind::Button {
            for &c in &self.nodes[self.center].children {
                let a = self.nodes[c].angle;
                candidates.push((c, a, dot(d, dir(a, 1.0))));
            }
        }
        if let Some(p) = self.parent_of_center() {
            let a = (self.nodes[self.center].angle + 180.0) % 360.0;
            candidates.push((p, a, dot(d, dir(a, 1.0))));
        }
        if candidates.is_empty() {
            return;
        }

        let limit = 45f32.to_radians().cos();
        let mut extreme: Option<(usize, f32, f32)> = None;
        for &c in &candidates {
            if c.2 > limit {
                match extreme {
                    None => extreme = Some(c),
                    Some(e) if c.2 > dot(d, dir(e.1, 1.0)) => extreme = Some(c),
                    _ => {}
                }
            }
        }
        if self.hovered.is_some() && self.hovered == extreme.map(|e| e.0) {
            return;
        }
        let Some(hovered) = self.hovered.filter(|&h| h != self.center) else {
            if let Some(e) = extreme {
                self.hover_angle(e.1);
            }
            return;
        };

        let hovered_angle = if Some(hovered) == self.parent_of_center() {
            (self.nodes[self.center].angle + 180.0) % 360.0
        } else {
            self.nodes[hovered].angle
        };
        let hovered_dir = dir(hovered_angle, 1.0);

        if let Some(e) = extreme {
            if dot(v(-d.x, -d.y), hovered_dir) > 20f32.to_radians().cos() {
                self.hover_angle(e.1);
                return;
            }
        }

        if candidates.len() > 1 {
            let cw = clockwise(hovered_dir, d);
            let mut best = None;
            let mut best_diff = 360.0;
            for &(item, a, _) in &candidates {
                if item == hovered {
                    continue;
                }
                let cd = dir(a, 1.0);
                let diff = angle_diff(hovered_angle, a);
                if (clockwise(hovered_dir, cd) == cw || dot(hovered_dir, cd) < 160f32.to_radians().cos()) && diff < best_diff {
                    best = Some(a);
                    best_diff = diff;
                }
            }
            if let Some(a) = best {
                self.hover_angle(a);
            }
        }
    }
}

impl Pie {
    pub fn stick(&mut self, x: f32, y: f32) {
        let rel = v(x, y) * self.parent_distance;
        let center = self.center_position();
        self.input = Input { button: Button::Released, abs: center + rel, rel, distance: len(rel), angle: angle_of(rel) };
        self.redraw();
    }

    pub fn pad_button(&mut self, index: usize, back: i64, close: i64) {
        if close >= 0 && index as i64 == close {
            self.hide();
            return;
        }
        let at = self.center_position();
        if back >= 0 && index as i64 == back {
            self.selection(at, Pick::Parent);
            return;
        }
        self.selection(at, Pick::Active);
    }

    pub fn has_submenu_children(&self) -> bool {
        self.nodes[self.center].children.iter().any(|&c| self.kind(c) == Kind::Submenu)
    }
}
