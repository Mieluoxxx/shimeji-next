//! 桌面环境模型：屏幕 / 工作区 / 活动窗口（原版叫 IE）/ 光标。
//! 全部使用逻辑坐标（macOS points，主屏左上为原点，y 向下）——原版配置的速度、
//! 偏移量都是按 1x 像素写的，物理像素只在渲染层出现。

use crate::platform::Platform;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.w > 0 && self.h > 0 && o.w > 0 && o.h > 0
            && self.x < o.right()
            && o.x < self.right()
            && self.y < o.bottom()
            && o.y < self.bottom()
    }
    pub fn contains_rect(&self, o: &Rect) -> bool {
        self.w > 0 && self.h > 0 && o.x >= self.x && o.y >= self.y && o.right() <= self.right() && o.bottom() <= self.bottom()
    }
    pub fn union(&self, o: &Rect) -> Rect {
        if self.w <= 0 || self.h <= 0 {
            return *o;
        }
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect {
            x,
            y,
            w: self.right().max(o.right()) - x,
            h: self.bottom().max(o.bottom()) - y,
        }
    }
}

/// d* 是上一 tick 到这一 tick 的边位移；Border::move 靠它把站在窗口上的宠物跟着窗口带走
#[derive(Debug, Clone, Default)]
pub struct Area {
    pub visible: bool,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub dleft: i32,
    pub dtop: i32,
    pub dright: i32,
    pub dbottom: i32,
}

impl Area {
    pub fn new() -> Self {
        Area {
            visible: true,
            ..Default::default()
        }
    }

    pub fn set(&mut self, r: Rect) {
        self.dleft = r.x - self.left;
        self.dtop = r.y - self.top;
        self.dright = r.right() - self.right;
        self.dbottom = r.bottom() - self.bottom;
        self.left = r.x;
        self.top = r.y;
        self.right = r.right();
        self.bottom = r.bottom();
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.left <= x && x <= self.right && self.top <= y && y <= self.bottom
    }
    pub fn rect(&self) -> Rect {
        Rect {
            x: self.left,
            y: self.top,
            w: self.width(),
            h: self.height(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaId {
    WorkArea,
    ActiveIe,
    Screen(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

/// Java 里 Border 是指向 Area 的对象引用；这里存 Area 的身份，每次按当前环境求值，
/// 活动窗口换了，同一个 Border 也会跟着变（与原版一致）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Border {
    NotOn,
    Edge(AreaId, Side),
}

/// 光标位置与平滑后的速度（像素/秒），松手扔出时作为初速度
#[derive(Debug, Clone, Copy, Default)]
pub struct Cursor {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
}

/// 速度平滑的时间常数：原版每 40ms 取 (旧速度+新位移)/2，相当于约 60ms 的指数平滑
const CURSOR_SMOOTHING: f64 = 0.06;

pub struct Environment {
    pub screen: Area,
    pub screens: Vec<(u32, Area)>,
    pub work_area: Area,
    pub active_ie: Area,
    pub ie_title: String,
    pub cursor: Cursor,
    pub multiscreen: bool,
}

/// 每只宠物独立缓存“当前所在工作区”，跨屏移动时由 work_area() 切换
#[derive(Debug, Clone, Default)]
pub struct MascotEnv {
    current: Option<AreaId>,
}

impl Environment {
    pub fn new() -> Self {
        Environment {
            screen: Area::new(),
            screens: Vec::new(),
            work_area: Area::new(),
            active_ie: Area::new(),
            ie_title: String::new(),
            cursor: Cursor::default(),
            multiscreen: true,
        }
    }

    /// poll_window=false 时沿用上次的窗口矩形（并清零位移），CGWindowList 不必每帧查
    pub fn tick(&mut self, platform: &mut dyn Platform, dt: f64, poll_window: bool) {
        let screens = platform.screens();
        let mut union = Rect::default();
        for s in &screens {
            union = union.union(&s.frame);
        }
        if union.w > 0 {
            self.screen.set(union);
        }
        self.screens.retain(|(id, _)| screens.iter().any(|s| s.id == *id));
        for s in &screens {
            match self.screens.iter_mut().find(|(id, _)| *id == s.id) {
                Some((_, area)) => area.set(s.visible),
                None => {
                    let mut area = Area::new();
                    area.set(s.visible);
                    self.screens.push((s.id, area));
                }
            }
        }
        if let Some(primary) = screens.iter().find(|s| s.primary).or(screens.first()) {
            self.work_area.set(primary.visible);
        }

        let (cx, cy) = platform.cursor();
        let (cx, cy) = (cx as f64, cy as f64);
        if dt > 0.0 {
            let a = 1.0 - (-dt / CURSOR_SMOOTHING).exp();
            let c = &mut self.cursor;
            c.vx += ((cx - c.x) / dt - c.vx) * a;
            c.vy += ((cy - c.y) / dt - c.vy) * a;
        }
        self.cursor.x = cx;
        self.cursor.y = cy;

        if !poll_window {
            let r = self.active_ie.rect();
            self.active_ie.set(r);
            return;
        }
        match platform.active_window(&screens) {
            Some(win) => {
                self.active_ie.visible = win.visible;
                self.active_ie.set(win.rect);
                self.ie_title = win.title;
            }
            None => {
                self.active_ie.visible = false;
                self.active_ie.set(Rect { x: -1, y: -1, w: 0, h: 0 });
                self.ie_title.clear();
            }
        }
    }

    pub fn area(&self, id: AreaId) -> Option<&Area> {
        match id {
            AreaId::WorkArea => Some(&self.work_area),
            AreaId::ActiveIe => Some(&self.active_ie),
            AreaId::Screen(sid) => self.screens.iter().find(|(i, _)| *i == sid).map(|(_, a)| a),
        }
    }

    pub fn is_on(&self, border: Border, p: (i32, i32)) -> bool {
        let Border::Edge(id, side) = border else {
            return false;
        };
        let Some(a) = self.area(id) else {
            return false;
        };
        edge_is_on(a, side, p)
    }

    /// FloorCeiling/Wall.move：按边的位移把锚点带走；位移过大视为窗口切换，不跟
    pub fn border_move(&self, border: Border, p: (i32, i32)) -> (i32, i32) {
        let Border::Edge(id, side) = border else {
            return p;
        };
        let Some(a) = self.area(id) else {
            return p;
        };
        if !a.visible {
            return p;
        }
        match side {
            Side::Top | Side::Bottom => {
                let d = a.right - a.dright - (a.left - a.dleft);
                if d == 0 {
                    return p;
                }
                let dy = if side == Side::Bottom { a.dbottom } else { a.dtop };
                // 原版 (right-left)/d 是 int 整除，宽度不变时恒为 1，照搬
                let nx = (p.0 - (a.left - a.dleft)) * ((a.right - a.left) / d) + a.left;
                let ny = p.1 + dy;
                if (nx - p.0).abs() >= 80 || ny - p.1 > 20 || ny - p.1 < -80 {
                    return p;
                }
                (nx, ny)
            }
            Side::Left | Side::Right => {
                let d = a.bottom - a.dbottom - (a.top - a.dtop);
                if d == 0 {
                    return p;
                }
                let dx = if side == Side::Right { a.dright } else { a.dleft };
                let nx = p.0 + dx;
                let ny = (p.1 - (a.top - a.dtop)) * (a.bottom - a.top) / d + a.top;
                if (nx - p.0).abs() >= 80 || (ny - p.1).abs() >= 80 {
                    return p;
                }
                (nx, ny)
            }
        }
    }

    fn is_screen_top_bottom(&self, p: (i32, i32)) -> bool {
        let mut count = 0;
        for (_, a) in &self.screens {
            if edge_is_on(a, Side::Top, p) {
                count += 1;
            }
            if edge_is_on(a, Side::Bottom, p) {
                count += 1;
            }
        }
        if count == 0 {
            return edge_is_on(&self.work_area, Side::Top, p) || edge_is_on(&self.work_area, Side::Bottom, p);
        }
        count == 1
    }

    fn is_screen_left_right(&self, p: (i32, i32)) -> bool {
        let mut count = 0;
        for (_, a) in &self.screens {
            if edge_is_on(a, Side::Left, p) {
                count += 1;
            }
            if edge_is_on(a, Side::Right, p) {
                count += 1;
            }
        }
        if count == 0 {
            return edge_is_on(&self.work_area, Side::Left, p) || edge_is_on(&self.work_area, Side::Right, p);
        }
        count == 1
    }

    /// MascotEnvironment.getWorkArea：优先主工作区，否则取锚点所在屏
    pub fn work_area_for(&self, me: &mut MascotEnv, anchor: (i32, i32), ignore_settings: bool) -> AreaId {
        if let Some(cur) = me.current {
            match self.area(cur) {
                Some(cur_area) if ignore_settings || self.multiscreen => {
                    if cur != AreaId::WorkArea
                        && cur_area.rect().contains_rect(&self.work_area.rect())
                        && self.work_area.contains(anchor.0, anchor.1)
                    {
                        me.current = Some(AreaId::WorkArea);
                        return AreaId::WorkArea;
                    }
                    if cur_area.contains(anchor.0, anchor.1) {
                        return cur;
                    }
                }
                Some(_) => return cur,
                None => {}
            }
        }
        if self.work_area.contains(anchor.0, anchor.1) {
            me.current = Some(AreaId::WorkArea);
            return AreaId::WorkArea;
        }
        for (id, a) in &self.screens {
            if a.contains(anchor.0, anchor.1) {
                me.current = Some(AreaId::Screen(*id));
                return AreaId::Screen(*id);
            }
        }
        me.current = Some(AreaId::WorkArea);
        AreaId::WorkArea
    }

    pub fn floor(&self, me: &mut MascotEnv, anchor: (i32, i32), ignore_separator: bool) -> Border {
        let ie = Border::Edge(AreaId::ActiveIe, Side::Top);
        if self.is_on(ie, anchor) {
            return ie;
        }
        let wa = Border::Edge(self.work_area_for(me, anchor, false), Side::Bottom);
        if self.is_on(wa, anchor) && (!ignore_separator || self.is_screen_top_bottom(anchor)) {
            return wa;
        }
        Border::NotOn
    }

    pub fn ceiling(&self, me: &mut MascotEnv, anchor: (i32, i32), ignore_separator: bool) -> Border {
        let ie = Border::Edge(AreaId::ActiveIe, Side::Bottom);
        if self.is_on(ie, anchor) {
            return ie;
        }
        let wa = Border::Edge(self.work_area_for(me, anchor, false), Side::Top);
        if self.is_on(wa, anchor) && (!ignore_separator || self.is_screen_top_bottom(anchor)) {
            return wa;
        }
        Border::NotOn
    }

    pub fn wall(&self, me: &mut MascotEnv, anchor: (i32, i32), look_right: bool, ignore_separator: bool) -> Border {
        let (ie_side, wa_side) = if look_right {
            (Side::Left, Side::Right)
        } else {
            (Side::Right, Side::Left)
        };
        let ie = Border::Edge(AreaId::ActiveIe, ie_side);
        if self.is_on(ie, anchor) {
            return ie;
        }
        let wa = Border::Edge(self.work_area_for(me, anchor, false), wa_side);
        if self.is_on(wa, anchor) && (!ignore_separator || self.is_screen_left_right(anchor)) {
            return wa;
        }
        Border::NotOn
    }
}

/// Java 版为高 DPI 加了 ±2 容差，但那会让落地后悬空 1~2 像素；
/// 这里全程逻辑坐标无舍入误差，恢复上游的精确相等
pub fn edge_is_on(a: &Area, side: Side, p: (i32, i32)) -> bool {
    if !a.visible {
        return false;
    }
    match side {
        Side::Top => a.top == p.1 && a.left <= p.0 && p.0 <= a.right,
        Side::Bottom => a.bottom == p.1 && a.left <= p.0 && p.0 <= a.right,
        Side::Left => a.left == p.0 && a.top <= p.1 && p.1 <= a.bottom,
        Side::Right => a.right == p.0 && a.top <= p.1 && p.1 <= a.bottom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_carries_mascot_with_window() {
        let mut env = Environment::new();
        env.active_ie.set(Rect { x: 100, y: 300, w: 400, h: 300 });
        env.active_ie.set(Rect { x: 110, y: 290, w: 400, h: 300 });
        let b = Border::Edge(AreaId::ActiveIe, Side::Top);
        assert_eq!(env.border_move(b, (200, 300)), (210, 290));
        assert!(env.is_on(b, (210, 290)));
    }

    #[test]
    fn large_jump_is_not_followed() {
        let mut env = Environment::new();
        env.active_ie.set(Rect { x: 100, y: 300, w: 400, h: 300 });
        env.active_ie.set(Rect { x: 600, y: 300, w: 400, h: 300 });
        let b = Border::Edge(AreaId::ActiveIe, Side::Top);
        assert_eq!(env.border_move(b, (200, 300)), (200, 300));
    }
}
