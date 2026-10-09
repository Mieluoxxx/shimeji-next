//! 无窗口仿真：假平台（1920x1080，菜单栏 25、Dock 55，一个可被扔的前台窗口）驱动真实引擎与全部内置皮肤。

use shimeji_next::assets::Resources;
use shimeji_next::platform::{Platform, ScreenInfo, WindowInfo};
use shimeji_next::settings::Settings;
use shimeji_next::sim::env::Rect;
use shimeji_next::sim::World;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

const FLOOR: f64 = 1025.0;
const DT: f64 = 1.0 / 60.0;

#[derive(Clone, Default)]
struct Shared {
    cursor: Rc<Cell<(i32, i32)>>,
    ie: Rc<RefCell<Option<Rect>>>,
}

struct Fake(Shared);

impl Platform for Fake {
    fn screens(&mut self) -> Vec<ScreenInfo> {
        vec![ScreenInfo {
            id: 0,
            frame: Rect { x: 0, y: 0, w: 1920, h: 1080 },
            visible: Rect { x: 0, y: 25, w: 1920, h: FLOOR as i32 - 25 },
            primary: true,
        }]
    }
    fn cursor(&mut self) -> (i32, i32) {
        self.0.cursor.get()
    }
    fn active_window(&mut self, _s: &[ScreenInfo]) -> Option<WindowInfo> {
        self.0.ie.borrow().map(|rect| WindowInfo { rect, visible: rect.x + rect.w > 0 && rect.x < 1920, title: "fake".into() })
    }
    fn can_move_windows(&mut self) -> bool {
        true
    }
    fn move_active_window(&mut self, x: i32, y: i32, _c: Rect, _b: Rect) {
        if let Some(r) = self.0.ie.borrow_mut().as_mut() {
            r.x = x;
            r.y = y;
        }
    }
    fn restore_windows(&mut self, _b: Rect) {}
}

fn assets() -> Resources {
    Resources::new(&std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets")))
}

fn world_with(shared: &Shared, res: Resources) -> World {
    let mut w = World::new(res, Settings::ephemeral(), Box::new(Fake(shared.clone())), false);
    w.prime_env();
    w
}

fn world(shared: &Shared) -> World {
    world_with(shared, assets())
}

/// spawn 进的是待加入队列，下一 tick 才生效
fn only(w: &mut World) -> u32 {
    w.tick(DT);
    w.live().next().expect("至少一只").id
}

fn state(w: &World, id: u32) -> (&'static str, f64, f64) {
    let m = w.mascot(id).unwrap();
    (m.behavior_name(), m.x, m.y)
}

fn run_secs(w: &mut World, secs: f64) {
    for _ in 0..(secs / DT) as usize {
        w.tick(DT);
    }
}

fn landed(w: &mut World, id: u32, max_secs: f64) -> f64 {
    let mut t = 0.0;
    while t < max_secs {
        w.tick(DT);
        t += DT;
        if state(w, id).2 == FLOOR {
            return t;
        }
    }
    panic!("{max_secs}s 内没落地: {:?}", state(w, id));
}

#[test]
fn every_skin_loads_and_runs() {
    let shared = Shared::default();
    shared.cursor.set((900, 600));
    *shared.ie.borrow_mut() = Some(Rect { x: 500, y: 400, w: 700, h: 400 });
    let mut w = world(&shared);
    let skins = w.res.list_skins();
    assert!(skins.contains(&"Shimeji".to_string()), "{skins:?}");
    for s in &skins {
        for _ in 0..4 {
            w.spawn(s);
        }
    }
    for _ in 0..60 * 90 {
        w.tick(DT);
        for m in w.live() {
            assert!(m.x.is_finite() && m.y.is_finite(), "{} 坐标异常", m.skin);
            assert!(m.x > -3000.0 && m.x < 5000.0 && m.y > -3000.0 && m.y < 3000.0, "{} 飞了: {} {}", m.skin, m.x, m.y);
            assert!(m.image.is_some() || m.y < 0.0, "{} 没有图像", m.skin);
        }
    }
    assert!(w.live().count() >= skins.len() * 4);
}

#[test]
fn walks_at_original_speed() {
    let shared = Shared::default();
    let mut w = world(&shared);
    w.spawn("Shimeji");
    let id = only(&mut w);
    landed(&mut w, id, 10.0);
    w.with_mascot(id, |m, _| m.x = 1500.0);
    w.set_behavior_of(id, "GrabWorkAreaBottomLeftWall");
    w.tick(DT);
    let x0 = state(&w, id).1;
    run_secs(&mut w, 4.0);
    let v = (x0 - state(&w, id).1) / 4.0;
    // 原版 Walk：2 像素/tick = 50 像素/秒
    assert!((v - 50.0).abs() < 2.0, "步行速度 {v} 像素/秒");
}

#[test]
fn falling_is_frame_rate_independent() {
    let time_to_land = |dt: f64| {
        let shared = Shared::default();
        let mut w = world(&shared);
        w.spawn("Shimeji");
        let id = only(&mut w);
        w.with_mascot(id, |m, w| {
            m.x = 960.0;
            m.y = 100.0;
            w.start(m, "Fall");
        });
        let mut t = 0.0;
        while state(&w, id).2 < FLOOR && t < 10.0 {
            w.tick(dt);
            t += dt;
        }
        t
    };
    let t60 = time_to_land(1.0 / 60.0);
    let t240 = time_to_land(1.0 / 240.0);
    let t25 = time_to_land(1.0 / 25.0);
    assert!((t60 - t240).abs() < 0.03, "60Hz {t60} vs 240Hz {t240}");
    assert!((t60 - t25).abs() < 0.06, "60Hz {t60} vs 25Hz {t25}");
}

#[test]
fn explores_behaviors() {
    let shared = Shared::default();
    shared.cursor.set((300, 300));
    let mut w = world(&shared);
    w.settings.breeding = false;
    for _ in 0..4 {
        w.spawn("Shimeji");
    }
    w.tick(DT);
    let ids: Vec<u32> = w.live().map(|m| m.id).collect();
    for &id in &ids {
        landed(&mut w, id, 10.0);
    }
    let mut seen: BTreeMap<&'static str, f64> = BTreeMap::new();
    for _ in 0..(60.0 * 60.0 * 20.0) as usize {
        w.tick(DT);
        for &id in &ids {
            let (b, _, y) = state(&w, id);
            assert!(y <= FLOOR, "掉到工作区下面: {b} {y}");
            *seen.entry(b).or_default() += DT;
        }
    }
    eprintln!("{seen:#?}");
    for b in ["Fall", "ClimbAlongWall", "ClimbAlongCeiling", "HoldOntoWall", "SitDown", "StandUp"] {
        assert!(seen.contains_key(b), "没出现 {b}");
    }
    assert!(seen.len() >= 15, "种类太少 {}", seen.len());
}

#[test]
fn drag_follows_cursor_every_frame_and_throws() {
    let shared = Shared::default();
    shared.cursor.set((800, 900));
    let mut w = world(&shared);
    w.spawn("Shimeji");
    let id = only(&mut w);
    landed(&mut w, id, 10.0);
    w.press(id);
    let mut frames = std::collections::HashSet::new();
    for i in 0..60 {
        let c = (800 + i * 12, 600 - i * 4);
        shared.cursor.set(c);
        w.tick(DT);
        let m = w.mascot(id).unwrap();
        assert!(m.is_dragged());
        assert_eq!((m.x, m.y), (c.0 as f64, c.1 as f64 + 120.0), "拎着时脚在光标正下方 120px");
        frames.insert(m.image.as_ref().unwrap().sprite.key.clone());
    }
    assert!(frames.len() >= 2, "甩动时应换姿势");
    w.release(id);
    assert_eq!(state(&w, id).0, "Thrown");
    let x_release = state(&w, id).1;
    run_secs(&mut w, 0.3);
    assert!(state(&w, id).1 > x_release + 50.0, "应带着光标速度向右飞出: {:?}", state(&w, id));
    run_secs(&mut w, 8.0);
    let (b, x, y) = state(&w, id);
    assert!(y == FLOOR || x == 0.0 || x == 1920.0, "扔出后应落地或抓墙: {b} {x} {y}");
}

#[test]
fn held_still_resists_then_escapes() {
    let shared = Shared::default();
    shared.cursor.set((800, 600));
    let mut w = world(&shared);
    w.spawn("Shimeji");
    let id = only(&mut w);
    landed(&mut w, id, 10.0);
    w.press(id);
    let mut t = 0.0;
    while state(&w, id).0 != "Fall" {
        w.tick(DT);
        t += DT;
        assert!(t < 25.0, "一直拎着不动应挣脱");
    }
    // 原版：静止 10 秒开始挣扎，挣扎动画 10.64 秒
    assert!((20.0..22.0).contains(&t), "挣脱用时 {t}");
}

#[test]
fn throws_active_window() {
    let shared = Shared::default();
    shared.cursor.set((1800, 200));
    *shared.ie.borrow_mut() = Some(Rect { x: 700, y: 300, w: 600, h: 400 });
    let mut w = world(&shared);
    w.settings.breeding = false;
    w.spawn("Shimeji");
    let id = only(&mut w);
    // 随机出生点可能正好落在窗口顶上，直接放到窗口左侧地面
    w.with_mascot(id, |m, w| {
        m.x = 300.0;
        m.y = FLOOR;
        w.start(m, "StandUp");
    });
    w.tick(DT);
    w.set_behavior_of(id, "ThrowIEFromLeft");
    let start = shared.ie.borrow().unwrap();
    run_secs(&mut w, 30.0);
    let end = shared.ie.borrow().unwrap();
    assert!(end.x > start.x + 300, "窗口应被扛走并扔出: {start:?} → {end:?}");
}

#[test]
fn splitting_spawns_a_child() {
    let shared = Shared::default();
    let mut w = world(&shared);
    w.spawn("Shimeji");
    let id = only(&mut w);
    landed(&mut w, id, 10.0);
    w.set_behavior_of(id, "SplitIntoTwo");
    run_secs(&mut w, 3.0);
    assert_eq!(w.live().count(), 2);
}

#[test]
fn user_skins_and_manifest() {
    let user = std::env::temp_dir().join(format!("shimeji-next-test-{}", std::process::id()));
    let mine = user.join("MySkin");
    std::fs::create_dir_all(&mine).unwrap();
    for e in std::fs::read_dir(assets().skins.join("Shimeji")).unwrap().flatten() {
        std::fs::copy(e.path(), mine.join(e.file_name())).unwrap();
    }
    std::fs::write(
        mine.join("skin.toml"),
        "name = \"我的皮肤\"\nscale = 2.0\n[behaviors]\ndisabled = [\"SplitIntoTwo\"]\n",
    )
    .unwrap();
    let res = assets().with_user_skins(user.clone());
    assert!(res.list_skins().contains(&"MySkin".to_string()));
    assert_eq!(res.display_name("MySkin"), "我的皮肤");
    assert_eq!(res.display_name("Shimeji"), "Shimeji");

    let shared = Shared::default();
    let mut w = world_with(&shared, res);
    w.spawn("MySkin");
    let id = only(&mut w);
    landed(&mut w, id, 10.0);
    let img = w.mascot(id).unwrap().image.clone().unwrap();
    assert_eq!(img.size(), (256, 256), "scale=2 应放大帧图");
    assert!(!w.behavior_enabled("MySkin", "SplitIntoTwo"));
    assert_eq!(w.skin("MySkin").unwrap().display_name(), "我的皮肤");
    std::fs::remove_dir_all(&user).ok();
}
