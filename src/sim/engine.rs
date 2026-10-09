//! 行为执行引擎：一个行为 = 一串 Step，按真实 dt 推进（秒、像素/秒），帧率无关。
//!
//! 语义沿用 Shimeji：带边界的动作（站/走/爬）每帧先跟随所贴的边（窗口被拖动时一起走），
//! 贴不住了就是“失足”，由上层切到下落；一个 Step 起步时就不满足（目标已到达等）直接跳过。

use super::clips::{self, Clip};
use super::env::{Area, Border};
use super::mascot::Mascot;
use super::world::World;

pub type Expr = Box<dyn Fn(&Sense) -> f64>;
pub type Cond = Box<dyn Fn(&Sense) -> bool>;

/// 原版 Fall 的阻力 0.05/0.1 每 tick、重力 2 像素/tick²，换算成连续的指数衰减：
/// 速度按 e^(-k·dt) 衰减，终端速度保持原版的 20 像素/tick（500 像素/秒）
const DRAG_X: f64 = 1.282_317_765;
const DRAG_Y: f64 = 2.634_013_002;
const TERMINAL_VY: f64 = 500.0;
/// 原版 Jump 的 VelocityParam=20 像素/tick
const JUMP_SPEED: f64 = 500.0;
/// 被拎起来时脚在光标下方的距离（原版 Dragged.OffsetY）
pub const PINCH_OFFSET_Y: f64 = 120.0;
/// 静止拎着多久开始挣扎（原版 250 tick）
const RESIST_AFTER: f64 = 10.0;
/// 扛着窗口时容许的错位：窗口位置是异步回报的，原版逐像素比对在 60fps 下会误判
const CARRY_TOLERANCE: i32 = 24;
const WINDOW_OFFSET_Y: i32 = -64;

/// 决策用的环境快照（替代原版 JS 表达式里的 mascot.environment.*）
#[derive(Clone)]
pub struct Sense {
    pub x: f64,
    pub y: f64,
    pub look_right: bool,
    pub wa: Area,
    pub ie: Area,
    pub screen: Area,
    pub cursor: (f64, f64),
    /// 像素/秒
    pub cursor_v: (f64, f64),
    pub total: usize,
}

impl Sense {
    fn p(&self) -> (i32, i32) {
        (self.x.round() as i32, self.y.round() as i32)
    }
    pub fn on_floor(&self) -> bool {
        super::env::edge_is_on(&self.wa, super::env::Side::Bottom, self.p())
    }
    pub fn on_ceiling(&self) -> bool {
        super::env::edge_is_on(&self.wa, super::env::Side::Top, self.p())
    }
    pub fn on_ie_top(&self) -> bool {
        super::env::edge_is_on(&self.ie, super::env::Side::Top, self.p())
    }
    pub fn on_ie_bottom(&self) -> bool {
        super::env::edge_is_on(&self.ie, super::env::Side::Bottom, self.p())
    }
    pub fn on_wa_left(&self) -> bool {
        super::env::edge_is_on(&self.wa, super::env::Side::Left, self.p())
    }
    pub fn on_wa_right(&self) -> bool {
        super::env::edge_is_on(&self.wa, super::env::Side::Right, self.p())
    }
    pub fn on_ie_left(&self) -> bool {
        super::env::edge_is_on(&self.ie, super::env::Side::Left, self.p())
    }
    pub fn on_ie_right(&self) -> bool {
        super::env::edge_is_on(&self.ie, super::env::Side::Right, self.p())
    }
    /// 面朝的屏幕边
    pub fn facing_wa_wall(&self) -> bool {
        if self.look_right {
            self.on_wa_right()
        } else {
            self.on_wa_left()
        }
    }
    /// 面朝的窗口侧边（朝右时贴的是窗口左边）
    pub fn facing_ie_wall(&self) -> bool {
        if self.look_right {
            self.on_ie_left()
        } else {
            self.on_ie_right()
        }
    }
    pub fn rand() -> f64 {
        rand::random::<f64>()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bk {
    None,
    Floor,
    Wall,
    Ceiling,
}

pub struct Tgt {
    f: Expr,
    /// true：每帧重算（原版 #{}），false：起步时算一次（原版 ${}）
    live: bool,
}

pub fn once(f: impl Fn(&Sense) -> f64 + 'static) -> Tgt {
    Tgt { f: Box::new(f), live: false }
}

pub fn live(f: impl Fn(&Sense) -> f64 + 'static) -> Tgt {
    Tgt { f: Box::new(f), live: true }
}

pub enum MoveClip {
    Fixed(&'static Clip),
    /// 往上爬/往下爬用不同剪辑，按目标在上还是在下选
    ClimbWall,
}

pub enum Step {
    Stay { clip: &'static Clip, border: Bk, dur: (f64, f64) },
    /// 坐着看鼠标：鼠标在屏幕上半时抬头
    LookAtMouse { dur: (f64, f64) },
    Move { clip: MoveClip, border: Bk, tx: Option<Tgt>, ty: Option<Tgt>, carry: bool },
    Animate { clip: &'static Clip, border: Bk },
    Fall { clip: &'static Clip, vx: Expr, vy: Expr, carry: bool },
    Jump { tx: Tgt, ty: Tgt },
    Look(Option<Cond>),
    Offset(Box<dyn Fn(&Sense) -> (f64, f64)>),
    ThrowWindow,
    Breed { clip: &'static Clip, born: (f64, f64), child: &'static str },
    /// 落地后：在地面/窗口顶上就弹一下再站会儿，否则（撞墙了）抓住墙
    Land,
    Chance(f64, Box<Step>),
    When(Cond, Vec<Step>),
    /// 依次判断，执行第一个满足条件的分支
    Branch(Vec<(Cond, Vec<Step>)>),
}

#[derive(Default)]
struct Run {
    t: f64,
    dur: f64,
    border: Option<Border>,
    tx: Option<f64>,
    ty: Option<f64>,
    vx: f64,
    vy: f64,
    spawned: bool,
    finished: bool,
}

pub enum Outcome {
    Running,
    Done,
    Lost,
}

pub struct Script {
    pub name: &'static str,
    steps: Vec<Step>,
    idx: usize,
    run: Option<Run>,
}

fn rand_range((a, b): (f64, f64)) -> f64 {
    if b > a {
        a + rand::random::<f64>() * (b - a)
    } else {
        a
    }
}

/// 原版目标坐标经 Number.intValue() 截断为整数，到达判定靠精确相等，这里保持一致
fn int(v: f64) -> f64 {
    if v.is_finite() {
        v.trunc()
    } else {
        0.0
    }
}

impl Script {
    pub fn new(name: &'static str, steps: Vec<Step>) -> Self {
        Script { name, steps, idx: 0, run: None }
    }

    /// 宏步骤（Land/Chance/When/Branch）到达时才求值，展开成具体步骤
    fn expand(&mut self, w: &World, m: &mut Mascot) -> bool {
        let Some(step) = self.steps.get(self.idx) else { return false };
        if !matches!(step, Step::Land | Step::Chance(..) | Step::When(..) | Step::Branch(..)) {
            return false;
        }
        let s = w.sense(m);
        let step = std::mem::replace(&mut self.steps[self.idx], Step::Land);
        let expansion = match step {
            Step::Land => {
                if s.on_floor() || s.on_ie_top() {
                    vec![
                        Step::Animate { clip: &clips::BOUNCING, border: Bk::Floor },
                        Step::Stay { clip: &clips::STAND, border: Bk::Floor, dur: (clips::ticks(100.0), clips::ticks(200.0)) },
                    ]
                } else {
                    vec![Step::Stay { clip: &clips::GRAB_WALL, border: Bk::Wall, dur: (clips::ticks(100.0), clips::ticks(100.0)) }]
                }
            }
            Step::Chance(p, inner) => {
                if rand::random::<f64>() < p {
                    vec![*inner]
                } else {
                    vec![]
                }
            }
            Step::When(c, body) => {
                if c(&s) {
                    body
                } else {
                    vec![]
                }
            }
            Step::Branch(arms) => arms.into_iter().find(|(c, _)| c(&s)).map(|(_, b)| b).unwrap_or_default(),
            other => vec![other],
        };
        self.steps.splice(self.idx..=self.idx, expansion);
        true
    }

    fn start(&mut self, m: &mut Mascot, w: &mut World) {
        let s = w.sense(m);
        let p = m.p();
        let mut r = Run::default();
        let border_of = |bk: Bk, m: &mut Mascot, w: &World| match bk {
            Bk::None => None,
            Bk::Floor => Some(w.env.floor(&mut m.env, p, false)),
            Bk::Ceiling => Some(w.env.ceiling(&mut m.env, p, false)),
            Bk::Wall => Some(w.env.wall(&mut m.env, p, m.look_right, false)),
        };
        match &self.steps[self.idx] {
            Step::Stay { border, dur, .. } => {
                r.border = border_of(*border, m, w);
                r.dur = rand_range(*dur);
            }
            Step::LookAtMouse { dur } => {
                r.border = border_of(Bk::Floor, m, w);
                r.dur = rand_range(*dur);
            }
            Step::Move { border, tx, ty, .. } => {
                r.border = border_of(*border, m, w);
                r.tx = tx.as_ref().map(|t| int((t.f)(&s)));
                r.ty = ty.as_ref().map(|t| int((t.f)(&s)));
            }
            Step::Animate { border, .. } => r.border = border_of(*border, m, w),
            Step::Fall { vx, vy, .. } => {
                r.vx = vx(&s);
                r.vy = vy(&s);
            }
            Step::Jump { tx, ty } => {
                r.tx = Some(int((tx.f)(&s)));
                r.ty = Some(int((ty.f)(&s)));
            }
            Step::Look(c) => {
                m.look_right = match c {
                    Some(c) => c(&s),
                    None => !m.look_right,
                };
                r.finished = true;
            }
            Step::Offset(f) => {
                let (dx, dy) = f(&s);
                m.x += dx.trunc();
                m.y += dy.trunc();
                r.finished = true;
            }
            Step::Breed { .. } | Step::ThrowWindow => {}
            Step::Land | Step::Chance(..) | Step::When(..) | Step::Branch(..) => r.finished = true,
        }
        self.run = Some(r);
    }

    fn has_next(&self, m: &mut Mascot, w: &mut World) -> bool {
        let Some(r) = &self.run else { return false };
        if r.finished {
            return false;
        }
        match &self.steps[self.idx] {
            Step::Stay { .. } | Step::LookAtMouse { .. } => r.t < r.dur,
            Step::Move { .. } => {
                let s = w.sense(m);
                let tx = self.target(0, &s);
                let ty = self.target(1, &s);
                let reached = tx.is_some_and(|x| m.x == x) || ty.is_some_and(|y| m.y == y);
                !reached
            }
            Step::Animate { clip, .. } | Step::Breed { clip, .. } => r.t < clip.duration(),
            Step::Fall { carry, .. } => {
                if *carry && !w.throwing_enabled() {
                    return false;
                }
                let p = m.p();
                let floor = w.env.floor(&mut m.env, p, false);
                let wall = w.env.wall(&mut m.env, p, m.look_right, false);
                !(w.env.is_on(floor, p) || w.env.is_on(wall, p))
            }
            Step::Jump { .. } => {
                let (tx, ty) = (r.tx.unwrap_or(m.x), r.ty.unwrap_or(m.y));
                let dx = tx - m.x;
                let dy = ty - m.y - dx.abs() / 2.0;
                (dx * dx + dy * dy).sqrt() != 0.0
            }
            Step::ThrowWindow => {
                r.t < clips::THROW_WINDOW.duration() && w.throwing_enabled() && w.env.active_ie.visible
            }
            _ => false,
        }
    }

    /// 0=x 1=y；live 目标每帧重算
    fn target(&self, axis: usize, s: &Sense) -> Option<f64> {
        let r = self.run.as_ref()?;
        let (stored, tgt) = match &self.steps[self.idx] {
            Step::Move { tx, ty, .. } => {
                if axis == 0 {
                    (r.tx, tx.as_ref())
                } else {
                    (r.ty, ty.as_ref())
                }
            }
            _ => return None,
        };
        match tgt {
            Some(t) if t.live => Some(int((t.f)(s))),
            _ => stored,
        }
    }

    fn border(&self) -> Option<Border> {
        self.run.as_ref().and_then(|r| r.border)
    }

    /// 贴边动作：先随边移动（窗口被拖走时一起走），再检查是否还贴得住
    fn follow_border(&self, m: &mut Mascot, w: &World) -> bool {
        let Some(b) = self.border() else { return true };
        let p = m.p();
        let q = w.env.border_move(b, p);
        m.x += (q.0 - p.0) as f64;
        m.y += (q.1 - p.1) as f64;
        w.env.is_on(b, m.p())
    }

    pub fn update(&mut self, m: &mut Mascot, w: &mut World, dt: f64) -> Outcome {
        // 一帧内可能连续跳过多个不可执行/瞬时的步骤（Look、Offset、已到达的移动…）
        let mut guard = self.steps.len() * 2 + 8;
        loop {
            if guard == 0 {
                return Outcome::Done;
            }
            guard -= 1;
            if self.idx >= self.steps.len() {
                return Outcome::Done;
            }
            if self.run.is_none() {
                if self.expand(w, m) {
                    continue;
                }
                self.start(m, w);
            }
            if self.has_next(m, w) {
                break;
            }
            self.idx += 1;
            self.run = None;
        }
        match self.tick(m, w, dt) {
            Ok(()) => Outcome::Running,
            Err(()) => Outcome::Lost,
        }
    }

    fn tick(&mut self, m: &mut Mascot, w: &mut World, dt: f64) -> Result<(), ()> {
        let skin_speed = w.skin(&m.skin).map_or(1.0, |s| s.speed);
        let s_before = w.sense(m);
        let idx = self.idx;
        match &self.steps[idx] {
            Step::Stay { clip, .. } => {
                let clip = *clip;
                if !self.follow_border(m, w) {
                    return Err(());
                }
                let t = self.advance(dt);
                m.show(w, clip.pose_at(t));
            }
            Step::LookAtMouse { .. } => {
                if !self.follow_border(m, w) {
                    return Err(());
                }
                self.advance(dt);
                let clip = if s_before.cursor.1 < s_before.screen.height() as f64 / 2.0 {
                    &clips::SIT_LOOK_UP
                } else {
                    &clips::SIT
                };
                m.show(w, clip.pose_at(0.0));
            }
            Step::Animate { clip, .. } => {
                let clip = *clip;
                if !self.follow_border(m, w) {
                    return Err(());
                }
                let t = self.advance(dt);
                let pose = clip.pose_at(t);
                m.move_pose(pose.vx * dt, pose.vy * dt);
                m.show(w, pose);
            }
            Step::Breed { clip, born, child } => {
                if !self.follow_border(m, w) {
                    return Err(());
                }
                let (clip, born, child) = (*clip, *born, *child);
                let t = self.advance(dt);
                m.show(w, clip.pose_at(t));
                let r = self.run.as_mut().expect("已起步");
                // 原版在倒数第一个 tick 生出新个体，即动画结尾
                if !r.spawned && t + dt >= clip.duration() {
                    r.spawned = true;
                    if w.settings.breeding {
                        w.breed(m, born, child);
                    }
                }
            }
            Step::Move { clip, carry, .. } => {
                let carry = *carry;
                if carry && !self.window_attached(m, w) {
                    return Err(());
                }
                if !self.follow_border(m, w) {
                    return Err(());
                }
                let s = w.sense(m);
                let tx = self.target(0, &s);
                let ty = self.target(1, &s);
                if let Some(tx) = tx {
                    if m.x != tx {
                        m.look_right = m.x < tx;
                    }
                }
                let down = ty.is_some_and(|ty| m.y < ty);
                let clip: &Clip = match (clip, ty) {
                    (MoveClip::Fixed(c), _) => c,
                    (MoveClip::ClimbWall, Some(ty)) if ty >= m.y => &clips::CLIMB_WALL_DOWN,
                    (MoveClip::ClimbWall, _) => &clips::CLIMB_WALL_UP,
                };
                let t = self.advance(dt * skin_speed);
                let pose = clip.pose_at(t);
                m.move_pose(pose.vx * dt * skin_speed, pose.vy * dt * skin_speed);
                if let Some(tx) = tx {
                    if (m.look_right && m.x >= tx) || (!m.look_right && m.x <= tx) {
                        m.x = tx;
                    }
                }
                if let Some(ty) = ty {
                    if (down && m.y >= ty) || (!down && m.y <= ty) {
                        m.y = ty;
                    }
                }
                m.show(w, pose);
                if carry {
                    self.carry_window(m, w);
                }
            }
            Step::Fall { carry, clip, .. } => {
                let (carry, clip) = (*carry, *clip);
                if carry && !self.window_attached(m, w) {
                    return Err(());
                }
                self.fall(m, w, dt, clip);
                if carry {
                    self.carry_window(m, w);
                }
            }
            Step::Jump { .. } => {
                let r = self.run.as_ref().expect("已起步");
                let (tx, ty) = (r.tx.unwrap_or(m.x), r.ty.unwrap_or(m.y));
                m.look_right = m.x < tx;
                let dx = tx - m.x;
                // 瞄准目标上方 |dx|/2 处，走出抛物线似的弧
                let dy = ty - m.y - dx.abs() / 2.0;
                let dist = (dx * dx + dy * dy).sqrt();
                let step = JUMP_SPEED * dt;
                if dist <= step {
                    m.x = tx;
                    m.y = ty;
                } else {
                    m.x += step * dx / dist;
                    m.y += step * dy / dist;
                }
                self.advance(dt);
                m.show(w, clips::JUMPING.pose_at(0.0));
            }
            Step::ThrowWindow => {
                let t = self.advance(dt);
                m.show(w, clips::THROW_WINDOW.pose_at(t));
                let ie = w.env.active_ie.rect();
                // 原版：水平 32 像素/tick，竖直初速 -10、重力 0.5 像素/tick²
                let vx = clips::per_tick(32.0) * dt;
                let vy = (clips::per_tick(-10.0) + 0.5 / (0.04 * 0.04) * t) * dt;
                let x = if m.look_right { ie.x as f64 + vx } else { ie.x as f64 - vx };
                w.move_ie(x.round() as i32, (ie.y as f64 + vy).round() as i32);
            }
            _ => {}
        }
        Ok(())
    }

    fn advance(&mut self, dt: f64) -> f64 {
        let r = self.run.as_mut().expect("已起步");
        let t = r.t;
        r.t += dt;
        t
    }

    fn window_attached(&self, m: &Mascot, w: &mut World) -> bool {
        if !w.throwing_enabled() {
            return false;
        }
        let ie = &w.env.active_ie;
        if !ie.visible {
            return false;
        }
        let (x, y) = m.p();
        let ex = if m.look_right { ie.left } else { ie.right };
        (x - ex).abs() <= CARRY_TOLERANCE && (y + WINDOW_OFFSET_Y - ie.bottom).abs() <= CARRY_TOLERANCE
    }

    fn carry_window(&self, m: &Mascot, w: &mut World) {
        let ie = w.env.active_ie.clone();
        if !ie.visible {
            return;
        }
        let (x, y) = m.p();
        let ny = y + WINDOW_OFFSET_Y - ie.height();
        let nx = if m.look_right { x } else { x - ie.width() };
        w.move_ie(nx, ny);
    }

    /// 连续时间版的原版 Fall：指数阻力 + 重力，沿路径逐像素找落点，防止高速穿过地面/墙
    fn fall(&mut self, m: &mut Mascot, w: &mut World, dt: f64, clip: &'static Clip) {
        let r = self.run.as_mut().expect("已起步");
        if r.vx != 0.0 {
            m.look_right = r.vx > 0.0;
        }
        let (vx0, vy0) = (r.vx, r.vy);
        let ex = (-DRAG_X * dt).exp();
        let ey = (-DRAG_Y * dt).exp();
        r.vx = vx0 * ex;
        r.vy = TERMINAL_VY + (vy0 - TERMINAL_VY) * ey;
        // 解析积分：∫v dt
        let dx = vx0 * (1.0 - ex) / DRAG_X;
        let dy = TERMINAL_VY * dt + (vy0 - TERMINAL_VY) * (1.0 - ey) / DRAG_Y;
        r.t += dt;

        let (sx, sy) = (m.x, m.y);
        let n = dx.abs().max(dy.abs()).ceil().max(1.0) as i32;
        'outer: for i in 1..=n {
            let k = i as f64 / n as f64;
            m.x = sx + dx * k;
            m.y = sy + dy * k;
            let p = m.p();
            if dy > 0.0 {
                // 原版 "HACK IE"：向上回溯找地面，窗口顶边正在上移时也能接住
                for j in -80..=0 {
                    let q = (p.0, p.1 + j);
                    let floor = w.env.floor(&mut m.env, q, true);
                    if w.env.is_on(floor, q) {
                        m.x = q.0 as f64;
                        m.y = q.1 as f64;
                        break 'outer;
                    }
                }
            }
            let wall = w.env.wall(&mut m.env, p, m.look_right, true);
            if w.env.is_on(wall, p) {
                m.x = p.0 as f64;
                m.y = p.1 as f64;
                break;
            }
        }
        m.show(w, clip.pose_at(0.0));
    }
}

/// 被拎起来的状态：脚是带阻尼的弹簧，甩动幅度决定姿势；静止太久开始挣扎，挣扎完挣脱掉下去
pub struct Drag {
    foot_x: f64,
    foot_dx: f64,
    still: f64,
    resist: Option<f64>,
    last_cursor_x: f64,
}

impl Drag {
    pub fn new(cursor_x: f64) -> Self {
        Drag {
            foot_x: cursor_x,
            foot_dx: 0.0,
            still: 0.0,
            resist: None,
            last_cursor_x: cursor_x,
        }
    }

    pub fn update(&mut self, m: &mut Mascot, w: &mut World, dt: f64) -> Outcome {
        let (cx, cy) = w.cursor();
        let scale = w.skin(&m.skin).map_or(1.0, |s| s.scale);

        if let Some(t) = self.resist.as_mut() {
            // 原版 Regist：光标离开挣扎位置 5 像素就停止挣扎，回到被拎着
            if (cx - m.x).abs() >= 5.0 {
                self.resist = None;
                self.still = 0.0;
            } else {
                let pose = clips::RESISTING.pose_at(*t);
                *t += dt;
                m.show(w, pose);
                if *t >= clips::RESISTING.duration() {
                    m.look_right = rand::random::<f64>() < 0.5;
                    return Outcome::Lost;
                }
                return Outcome::Running;
            }
        }

        m.look_right = false;
        let s = dt / 0.04;
        // 原版每 tick 移动 ≥5 像素算“在动”，按帧长等比缩放阈值
        if (cx - self.last_cursor_x).abs() >= 5.0 * s {
            self.still = 0.0;
        } else {
            self.still += dt;
        }
        self.last_cursor_x = cx;
        self.foot_dx = (self.foot_dx + (cx - self.foot_x) * 0.1 * s) * 0.8f64.powf(s);
        self.foot_x += self.foot_dx * s;
        m.x = cx;
        m.y = cy + (PINCH_OFFSET_Y * scale).round();
        m.show_frame(w, clips::pinched_frame(self.foot_x, cx), None);
        if self.still >= RESIST_AFTER {
            self.resist = Some(0.0);
        }
        Outcome::Running
    }
}
