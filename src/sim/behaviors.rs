//! 行为表：Shimeji-ee 默认 behaviors.xml + actions.xml 的全部行为，改写成类型化的 Rust。
//! 权重、隐藏、后续行为链与原版一致；原版里 `Math.random*100`（少了括号得到 NaN）这类笔误按本意修正。

use super::clips::{self, ticks, Clip};
use super::engine::{live, once, Bk, MoveClip, Sense, Step, Tgt};
use std::cell::Cell;
use std::rc::Rc;

pub struct NextRef {
    pub name: &'static str,
    pub weight: u32,
    pub when: Option<fn(&Sense) -> bool>,
}

pub struct Next {
    /// false：只从后续列表里挑；true：后续列表加进常规候选
    pub additive: bool,
    pub refs: Vec<NextRef>,
}

pub struct BehaviorDef {
    pub name: &'static str,
    pub weight: u32,
    /// 不出现在右键「触发行为」菜单
    pub hidden: bool,
    pub when: fn(&Sense) -> bool,
    pub build: fn(&Sense) -> Vec<Step>,
    pub next: Option<Next>,
}

/// 这几个由引擎直接调用，不参与随机挑选，也不能被用户禁用
pub const REQUIRED: [&str; 3] = ["Fall", "Thrown", "Dragged"];

// ---------- 条件 ----------

fn always(_: &Sense) -> bool {
    true
}
fn floorish(s: &Sense) -> bool {
    s.on_floor() || s.on_ie_top()
}
fn wall_group(s: &Sense) -> bool {
    s.facing_wa_wall() || s.facing_ie_wall()
}
fn ceiling_group(s: &Sense) -> bool {
    s.on_ceiling() || s.on_ie_bottom()
}
fn on_floor(s: &Sense) -> bool {
    s.on_floor()
}
fn on_ceiling(s: &Sense) -> bool {
    s.on_ceiling()
}
fn on_ie_top(s: &Sense) -> bool {
    s.on_ie_top()
}
fn facing_ie_wall(s: &Sense) -> bool {
    s.facing_ie_wall()
}
fn on_ie_bottom(s: &Sense) -> bool {
    s.on_ie_bottom()
}
fn facing_wa_wall(s: &Sense) -> bool {
    s.facing_wa_wall()
}
fn below_cap(s: &Sense) -> bool {
    s.total < 50
}

// ---------- 步骤构造 ----------

fn r() -> f64 {
    Sense::rand()
}
fn tk(a: f64, b: f64) -> (f64, f64) {
    (ticks(a), ticks(b))
}
const FOREVER: (f64, f64) = (f64::INFINITY, f64::INFINITY);

fn stay(clip: &'static Clip, border: Bk, dur: (f64, f64)) -> Step {
    Step::Stay { clip, border, dur }
}
fn stand(dur: (f64, f64)) -> Step {
    stay(&clips::STAND, Bk::Floor, dur)
}
fn sit(dur: (f64, f64)) -> Step {
    stay(&clips::SIT, Bk::Floor, dur)
}
fn mv(clip: &'static Clip, tx: Tgt) -> Step {
    Step::Move { clip: MoveClip::Fixed(clip), border: Bk::Floor, tx: Some(tx), ty: None, carry: false }
}
fn walk(tx: Tgt) -> Step {
    mv(&clips::WALK, tx)
}
fn run(tx: Tgt) -> Step {
    mv(&clips::RUN, tx)
}
fn dash(tx: Tgt) -> Step {
    mv(&clips::DASH, tx)
}
fn creep(tx: Tgt) -> Step {
    mv(&clips::CREEP, tx)
}
fn climb_wall(ty: Tgt) -> Step {
    Step::Move { clip: MoveClip::ClimbWall, border: Bk::Wall, tx: None, ty: Some(ty), carry: false }
}
fn climb_ceiling(tx: Tgt) -> Step {
    Step::Move { clip: MoveClip::Fixed(&clips::CLIMB_CEILING), border: Bk::Ceiling, tx: Some(tx), ty: None, carry: false }
}
fn carry_walk(clip: &'static Clip, tx: Tgt) -> Step {
    Step::Move { clip: MoveClip::Fixed(clip), border: Bk::Floor, tx: Some(tx), ty: None, carry: true }
}
/// 初速度用原版的 像素/tick 填，内部换算
fn falling(vx: impl Fn(&Sense) -> f64 + 'static, vy: impl Fn(&Sense) -> f64 + 'static) -> Step {
    Step::Fall {
        clip: &clips::FALLING,
        vx: Box::new(move |s| clips::per_tick(vx(s))),
        vy: Box::new(move |s| clips::per_tick(vy(s))),
        carry: false,
    }
}
fn bouncing() -> Step {
    Step::Animate { clip: &clips::BOUNCING, border: Bk::Floor }
}
fn jumping(tx: impl Fn(&Sense) -> f64 + 'static, ty: impl Fn(&Sense) -> f64 + 'static) -> Step {
    Step::Jump { tx: once(tx), ty: once(ty) }
}
fn look(lr: Option<fn(&Sense) -> bool>) -> Step {
    Step::Look(lr.map(|f| Box::new(f) as Box<dyn Fn(&Sense) -> bool>))
}
fn offset(f: impl Fn(&Sense) -> (f64, f64) + 'static) -> Step {
    Step::Offset(Box::new(f))
}
fn grab_wall(dur: (f64, f64)) -> Step {
    stay(&clips::GRAB_WALL, Bk::Wall, dur)
}
fn grab_ceiling(dur: (f64, f64)) -> Step {
    stay(&clips::GRAB_CEILING, Bk::Ceiling, dur)
}
fn wa(s: &Sense) -> (f64, f64, f64, f64) {
    (s.wa.left as f64, s.wa.top as f64, s.wa.right as f64, s.wa.bottom as f64)
}
fn ie(s: &Sense) -> (f64, f64, f64, f64) {
    (s.ie.left as f64, s.ie.top as f64, s.ie.right as f64, s.ie.bottom as f64)
}

fn sit_while_dangling_legs() -> Vec<Step> {
    vec![
        stay(&clips::SIT_LEGS_UP, Bk::Floor, tk(10.0, 10.0)),
        stay(&clips::SIT_LEGS_DOWN, Bk::Floor, tk(100.0, 200.0)),
        stay(&clips::SIT_DANGLE_LEGS, Bk::Floor, tk(500.0, 600.0)),
        stay(&clips::SIT_LEGS_DOWN, Bk::Floor, tk(100.0, 200.0)),
        stay(&clips::SIT_LEGS_UP, Bk::Floor, tk(10.0, 10.0)),
    ]
}

fn divided() -> Vec<Step> {
    vec![falling(|s| if s.look_right { 10.0 } else { -10.0 }, |_| -5.0), bouncing()]
}

/// 扛起窗口 → 走到屏幕另一侧 → 扔出去
fn throw_window(from_left: bool, walk_clip: &'static Clip) -> Vec<Step> {
    let edge = move |s: &Sense| if from_left { ie(s).0 } else { ie(s).2 };
    vec![
        jumping(edge, |s| ie(s).3 + 64.0),
        Step::Fall { clip: &clips::HOLD_WINDOW, vx: Box::new(|_| 0.0), vy: Box::new(|_| 0.0), carry: true },
        carry_walk(
            walk_clip,
            live(move |s| if from_left { wa(s).2 - 400.0 } else { wa(s).0 + 400.0 }),
        ),
        Step::ThrowWindow,
        stand(tk(50.0, 100.0)),
        look(None),
        stand(tk(50.0, 100.0)),
    ]
}

fn chase_mouse(_: &Sense) -> Vec<Step> {
    let drop_off = |dx: f64, dy: f64| vec![offset(move |_| (dx, dy)), falling(|_| 0.0, |_| 0.0), bouncing()];
    let ie_mid = |s: &Sense| (ie(s).0 + ie(s).2) / 2.0;
    let gap: Rc<Cell<Option<f64>>> = Rc::default();
    vec![
        Step::When(Box::new(ceiling_group), drop_off(0.0, 1.0)),
        Step::When(Box::new(|s: &Sense| s.on_wa_left() || s.on_ie_right()), drop_off(1.0, 0.0)),
        Step::When(Box::new(|s: &Sense| s.on_wa_right() || s.on_ie_left()), drop_off(-1.0, 0.0)),
        Step::Branch(vec![
            (
                Box::new(move |s: &Sense| s.on_ie_top() && s.x < ie_mid(s)),
                vec![
                    dash(once(|s| ie(s).0 + r() * 20.0 + 20.0)),
                    falling(|_| -5.0 - r() * 2.0, |_| -20.0 - r() * 5.0),
                    bouncing(),
                ],
            ),
            (
                Box::new(move |s: &Sense| s.on_ie_top() && s.x >= ie_mid(s)),
                vec![
                    dash(once(|s| ie(s).2 - r() * 20.0 - 20.0)),
                    falling(|_| 5.0 + r() * 2.0, |_| -20.0 - r() * 5.0),
                    bouncing(),
                ],
            ),
        ]),
        dash(once(|s| s.x + (s.cursor.0 - s.x) * r() / 2.0)),
        Step::Chance(0.05, Box::new(Step::Animate { clip: &clips::TRIPPING, border: Bk::Floor })),
        dash(once(|s| s.x + (s.cursor.0 - s.x) * r())),
        Step::Chance(0.05, Box::new(Step::Animate { clip: &clips::TRIPPING, border: Bk::Floor })),
        // 最后一段贴着光标停下，留出 0~200 像素的随机间隔（起步时定下，之后跟着光标走）
        dash(live(move |s| {
            let g = gap.get().unwrap_or_else(|| {
                let (x, cx) = (s.x, s.cursor.0);
                let g = if x < cx { -(cx - x).min(r() * 200.0) } else { (x - cx).min(r() * 200.0) };
                gap.set(Some(g));
                g
            });
            s.cursor.0 + g
        })),
        look(Some(|s| s.x < s.cursor.0)),
    ]
}

fn sit_and_face_mouse(_: &Sense) -> Vec<Step> {
    (0..10)
        .flat_map(|_| [Step::LookAtMouse { dur: tk(10.0, 20.0) }, look(Some(|s| s.x < s.cursor.0))])
        .collect()
}

macro_rules! b {
    ($name:literal, $w:expr, hidden, $when:expr, $build:expr $(, $next:expr)?) => {
        b!(@ $name, $w, true, $when, $build $(, $next)?)
    };
    ($name:literal, $w:expr, $when:expr, $build:expr $(, $next:expr)?) => {
        b!(@ $name, $w, false, $when, $build $(, $next)?)
    };
    (@ $name:literal, $w:expr, $hidden:expr, $when:expr, $build:expr) => {
        BehaviorDef { name: $name, weight: $w, hidden: $hidden, when: $when, build: $build, next: None }
    };
    (@ $name:literal, $w:expr, $hidden:expr, $when:expr, $build:expr, $next:expr) => {
        BehaviorDef { name: $name, weight: $w, hidden: $hidden, when: $when, build: $build, next: Some($next) }
    };
}

const fn nr(name: &'static str, weight: u32) -> NextRef {
    NextRef { name, weight, when: None }
}

pub fn table() -> Vec<BehaviorDef> {
    vec![
        // ---- 由引擎/菜单直接触发 ----
        b!("ChaseMouse", 0, hidden, always, chase_mouse,
            Next { additive: false, refs: vec![nr("SitAndFaceMouse", 1)] }),
        b!("SitAndFaceMouse", 0, always, sit_and_face_mouse,
            Next { additive: false, refs: vec![nr("SitAndFaceMouse", 100), nr("SitAndSpinHead", 1), nr("SitWhileDanglingLegs", 1)] }),
        b!("SitAndSpinHead", 0, always,
            |_| vec![Step::Animate { clip: &clips::SIT_SPIN_HEAD, border: Bk::Floor }],
            Next { additive: false, refs: vec![nr("SitAndFaceMouse", 1)] }),
        b!("Fall", 0, hidden, always, |_| vec![falling(|_| 0.0, |_| 0.0), Step::Land]),
        b!("Thrown", 0, hidden, always, |s| {
            // 光标速度（像素/秒）直接作为初速度
            let (vx, vy) = s.cursor_v;
            vec![
                Step::Fall { clip: &clips::FALLING, vx: Box::new(move |_| vx), vy: Box::new(move |_| vy), carry: false },
                Step::Land,
            ]
        }),
        b!("PullUp", 0, hidden, always,
            |_| vec![falling(|s| if s.look_right { 10.0 } else { -10.0 }, |_| -40.0), bouncing()]),
        b!("Divided", 0, hidden, always, |_| divided()),

        // ---- 在地面（含窗口顶上） ----
        b!("StandUp", 200, hidden, floorish, |_| vec![stand(tk(500.0, 1500.0))]),
        b!("SitDown", 200, floorish, |_| vec![sit(tk(500.0, 1500.0))],
            Next { additive: true, refs: vec![nr("SitWhileDanglingLegs", 100), nr("LieDown", 100)] }),
        b!("SitWhileDanglingLegs", 0, floorish, |_| sit_while_dangling_legs()),
        b!("LieDown", 0, floorish, |_| vec![stay(&clips::SPRAWL, Bk::Floor, tk(500.0, 1500.0))],
            Next { additive: false, refs: vec![
                nr("SitDown", 100),
                NextRef { name: "CrawlAlongIECeiling", weight: 100, when: Some(on_ie_top) },
                NextRef { name: "CrawlAlongWorkAreaFloor", weight: 100, when: Some(on_floor) },
            ] }),
        b!("SplitIntoTwo", 50, |s| floorish(s) && below_cap(s), |_| {
            let mut steps = vec![
                Step::Breed { clip: &clips::DIVIDE, born: (-16.0, 0.0), child: "Divided" },
                offset(|_| (16.0, 0.0)),
                look(None),
            ];
            steps.extend(divided());
            steps
        }),

        // ---- 贴在墙上 ----
        b!("HoldOntoWall", 100, hidden, wall_group, |_| vec![grab_wall(tk(500.0, 1500.0))]),
        b!("FallFromWall", 50, hidden, |s| wall_group(s) && !s.on_floor(),
            |_| vec![offset(|s| (if s.look_right { -1.0 } else { 1.0 }, 0.0)), stand(FOREVER)]),

        // ---- 挂在天花板 ----
        b!("HoldOntoCeiling", 100, hidden, ceiling_group, |_| vec![grab_ceiling(tk(500.0, 1500.0))]),
        b!("FallFromCeiling", 50, hidden, ceiling_group, |_| vec![offset(|_| (0.0, 1.0)), stand(FOREVER)]),

        // ---- 工作区地面（Dock 上沿） ----
        b!("WalkAlongWorkAreaFloor", 100, hidden, on_floor,
            |_| vec![walk(once(|s| wa(s).0 + 64.0 + r() * (s.wa.width() as f64 - 128.0)))]),
        b!("RunAlongWorkAreaFloor", 100, hidden, on_floor,
            |_| vec![run(once(|s| wa(s).0 + 64.0 + r() * (s.wa.width() as f64 - 128.0)))]),
        b!("CrawlAlongWorkAreaFloor", 10, on_floor,
            |_| vec![creep(once(|s| wa(s).0 + 64.0 + r() * (s.wa.width() as f64 - 128.0)))],
            Next { additive: false, refs: vec![nr("LieDown", 1)] }),
        b!("WalkLeftAlongFloorAndSit", 100, hidden, on_floor, |_| vec![
            walk(once(|s| wa(s).0 + 100.0 + r() * 300.0)),
            stand(tk(20.0, 40.0)), look(Some(|_| true)), stand(tk(20.0, 40.0)), sit(tk(500.0, 1500.0)),
        ]),
        b!("WalkRightAlongFloorAndSit", 100, hidden, on_floor, |_| vec![
            walk(once(|s| wa(s).2 - 100.0 - r() * 300.0)),
            stand(tk(20.0, 40.0)), look(Some(|_| false)), stand(tk(20.0, 40.0)), sit(tk(500.0, 1500.0)),
        ]),
        b!("GrabWorkAreaBottomLeftWall", 100, hidden, on_floor,
            |_| vec![walk(live(|s| wa(s).0)), climb_wall(live(|s| wa(s).3 - 64.0))]),
        b!("GrabWorkAreaBottomRightWall", 100, hidden, on_floor,
            |_| vec![walk(live(|s| wa(s).2)), climb_wall(live(|s| wa(s).3 - 64.0))]),
        b!("WalkLeftAndSit", 100, hidden, on_floor, |_| vec![
            run(once(|s| wa(s).0 + 100.0 + r() * 300.0)),
            stand(tk(20.0, 40.0)), look(Some(|_| true)), stand(tk(20.0, 40.0)), sit(tk(500.0, 1500.0)),
        ]),
        b!("WalkRightAndSit", 100, hidden, on_floor, |_| vec![
            run(once(|s| wa(s).2 - 100.0 - r() * 300.0)),
            stand(tk(20.0, 40.0)), look(Some(|_| false)), stand(tk(20.0, 40.0)), sit(tk(500.0, 1500.0)),
        ]),
        b!("WalkAndGrabBottomLeftWall", 100, on_floor,
            |_| vec![run(live(|s| wa(s).0)), climb_wall(live(|s| wa(s).3 - 64.0))]),
        b!("WalkAndGrabBottomRightWall", 100, on_floor,
            |_| vec![run(live(|s| wa(s).2)), climb_wall(live(|s| wa(s).3 - 64.0))]),
        b!("JumpFromBottomOfIE", 50, |s| s.on_floor() && s.x >= ie(s).0 && s.x < ie(s).2, |_| vec![
            jumping(|s| (s.x * 3.0 + ie(s).0 + r() * s.ie.width() as f64) / 4.0, |s| ie(s).3),
            grab_ceiling(tk(100.0, 200.0)),
        ]),
        b!("PullUpShimeji", 50, |s| s.on_floor() && below_cap(s), |_| vec![
            Step::Breed { clip: &clips::PULL_UP, born: (-32.0, 96.0), child: "PullUp" },
            Step::Animate { clip: &clips::PULL_UP_RECOIL, border: Bk::None },
            falling(|s| if s.look_right { -20.0 } else { 20.0 }, |_| 0.0),
            bouncing(),
            stay(&clips::SPRAWL, Bk::Floor, tk(40.0, 40.0)),
        ]),

        // ---- 面朝屏幕边 ----
        b!("ClimbHalfwayAlongWall", 100, hidden, facing_wa_wall,
            |_| vec![climb_wall(once(|s| wa(s).1 + 64.0 + r() * (s.wa.height() as f64 - 128.0)))]),
        b!("ClimbAlongWall", 100, facing_wa_wall, |_| vec![
            climb_wall(live(|s| wa(s).1 + 64.0)),
            offset(|_| (0.0, -64.0)),
            look(None),
            climb_ceiling(once(|s| if s.look_right { wa(s).0 + r() * 100.0 } else { wa(s).2 - r() * 100.0 })),
        ]),

        // ---- 屏幕顶（菜单栏下沿） ----
        b!("ClimbAlongCeiling", 100, on_ceiling,
            |_| vec![climb_ceiling(once(|s| wa(s).0 + 64.0 + r() * (s.wa.width() as f64 - 128.0)))]),

        // ---- 窗口顶上 ----
        b!("WalkAlongIECeiling", 100, hidden, on_ie_top,
            |_| vec![walk(once(|s| ie(s).0 + 64.0 + r() * (s.ie.width() as f64 - 128.0)))]),
        b!("RunAlongIECeiling", 100, hidden, on_ie_top,
            |_| vec![run(once(|s| ie(s).0 + 64.0 + r() * (s.ie.width() as f64 - 128.0)))]),
        b!("CrawlAlongIECeiling", 10, hidden, on_ie_top,
            |_| vec![creep(once(|s| ie(s).0 + 64.0 + r() * (s.ie.width() as f64 - 128.0)))],
            Next { additive: false, refs: vec![nr("LieDown", 1)] }),
        b!("SitOnTheLeftEdgeOfIE", 100, hidden, on_ie_top, |_| vec![
            walk(once(|s| ie(s).0 + 100.0 + r() * 300.0)), stand(tk(100.0, 200.0)), sit(tk(500.0, 1500.0)),
        ]),
        b!("SitOnTheRightEdgeOfIE", 100, hidden, on_ie_top, |_| vec![
            walk(once(|s| ie(s).2 - 100.0 - r() * 300.0)), stand(tk(100.0, 200.0)), sit(tk(500.0, 1500.0)),
        ]),
        b!("JumpFromLeftEdgeOfIE", 100, hidden, on_ie_top, |_| vec![
            walk(once(|s| ie(s).0 + r() * 50.0)), stand(tk(50.0, 100.0)),
            falling(|_| -15.0 - r() * 5.0, |_| -20.0 - r() * 5.0), bouncing(),
        ]),
        b!("JumpFromRightEdgeOfIE", 100, hidden, on_ie_top, |_| vec![
            walk(once(|s| ie(s).2 - r() * 50.0)), stand(tk(50.0, 100.0)),
            falling(|_| 15.0 + r() * 5.0, |_| -20.0 - r() * 5.0), bouncing(),
        ]),
        b!("WalkLeftAlongIEAndSit", 100, hidden, on_ie_top, |_| vec![
            run(once(|s| ie(s).0 + 100.0 + r() * 300.0)), stand(tk(100.0, 200.0)), sit(tk(500.0, 1500.0)),
        ]),
        b!("WalkRightAlongIEAndSit", 100, hidden, on_ie_top, |_| vec![
            run(once(|s| ie(s).2 - 100.0 - r() * 300.0)), stand(tk(100.0, 200.0)), sit(tk(500.0, 1500.0)),
        ]),
        b!("WalkLeftAlongIEAndJump", 100, hidden, on_ie_top, |_| vec![
            run(once(|s| ie(s).0 + r() * 50.0)),
            falling(|_| -15.0 - r() * 5.0, |_| -20.0 - r() * 5.0), bouncing(),
        ]),
        b!("WalkRightAlongIEAndJump", 100, hidden, on_ie_top, |_| vec![
            run(once(|s| ie(s).2 - r() * 50.0)),
            falling(|_| 15.0 + r() * 5.0, |_| -20.0 - r() * 5.0), bouncing(),
        ]),

        // ---- 窗口侧边 ----
        b!("HoldOntoIEWall", 100, hidden, facing_ie_wall,
            |_| vec![climb_wall(once(|s| ie(s).1 + 64.0 + r() * (s.ie.height() as f64 - 128.0)))]),
        b!("ClimbIEWall", 100, hidden, facing_ie_wall, |_| vec![
            climb_wall(live(|s| ie(s).1 + 64.0)),
            offset(|_| (0.0, -64.0)),
            sit(tk(5.0, 5.0)),
            stand(tk(10.0, 20.0)),
            walk(once(|s| if s.look_right { ie(s).0 + r() * 100.0 } else { ie(s).2 - r() * 100.0 })),
        ]),

        // ---- 窗口底边 ----
        b!("ClimbIEBottom", 100, hidden, on_ie_bottom,
            |_| vec![climb_ceiling(once(|s| ie(s).0 + 64.0 + (r() * s.ie.width() as f64 - 128.0)))]),
        b!("GrabIEBottomLeftWall", 100, hidden, on_ie_bottom,
            |_| vec![climb_ceiling(live(|s| ie(s).0)), look(None), climb_wall(live(|s| ie(s).3 - 64.0))]),
        b!("GrabIEBottomRightWall", 100, hidden, on_ie_bottom,
            |_| vec![climb_ceiling(live(|s| ie(s).2)), look(None), climb_wall(live(|s| ie(s).3 - 64.0))]),

        // ---- 跳到屏幕边 ----
        b!("JumpFromLeftWall", 50, hidden,
            |s| !s.on_wa_left() && s.x < wa(s).0 + 400.0 && (wa(s).3 - s.y).abs() < s.wa.height() as f64 / 4.0,
            |_| vec![jumping(|s| wa(s).0, |s| wa(s).3 - r() * s.wa.height() as f64 / 4.0), grab_wall(tk(100.0, 200.0))]),
        b!("JumpFromRightWall", 50, hidden,
            |s| !s.on_wa_right() && s.x >= wa(s).2 - 400.0 && (wa(s).3 - s.y).abs() < s.wa.height() as f64 / 4.0,
            |_| vec![jumping(|s| wa(s).2, |s| wa(s).3 - r() * s.wa.height() as f64 / 4.0), grab_wall(tk(100.0, 200.0))]),

        // ---- 有前台窗口时 ----
        b!("JumpOnIELeftWall", 50, hidden,
            |s| s.ie.visible && s.x < ie(s).0 && (ie(s).3 - s.y).abs() < s.ie.height() as f64 / 4.0,
            |_| vec![jumping(|s| ie(s).0, |s| ie(s).3 - r() * s.ie.height() as f64 / 4.0), grab_wall(tk(100.0, 200.0))]),
        b!("JumpOnIERightWall", 50, hidden,
            |s| s.ie.visible && s.x > ie(s).2 && (ie(s).3 - s.y).abs() < s.ie.height() as f64 / 4.0,
            |_| vec![jumping(|s| ie(s).2, |s| ie(s).3 - r() * s.ie.height() as f64 / 4.0), grab_wall(tk(100.0, 200.0))]),
        b!("ThrowIEFromLeft", 20, |s| s.ie.visible && ie(s).3 < s.y - 64.0 && s.x < ie(s).0,
            |_| throw_window(true, &clips::WALK_WITH_WINDOW)),
        b!("ThrowIEFromRight", 20, |s| s.ie.visible && ie(s).3 < s.y - 64.0 && s.x > ie(s).2,
            |_| throw_window(false, &clips::WALK_WITH_WINDOW)),
        b!("WalkAndThrowIEFromRight", 20, |s| s.ie.visible && ie(s).3 < s.y - 64.0 && s.x < ie(s).0,
            |_| throw_window(true, &clips::RUN_WITH_WINDOW)),
        b!("WalkAndThrowIEFromLeft", 20, |s| s.ie.visible && ie(s).3 < s.y - 64.0 && s.x > ie(s).2,
            |_| throw_window(false, &clips::RUN_WITH_WINDOW)),
    ]
}
