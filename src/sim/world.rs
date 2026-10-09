//! 全局状态与调度：行为挑选（条件 + 权重 + 后续行为链）、繁殖、拖拽、按 dt 推进全部宠物。
//!
//! tick 时把当前宠物从槽位里取出（Option::take），其余宠物仍可经 World 访问，
//! 无需 Rc<RefCell> 互指。

use super::behaviors::{self, BehaviorDef, REQUIRED};
use super::engine::{Drag, Outcome, Script, Sense};
use super::env::Environment;
use super::mascot::{Brain, Mascot};
use crate::assets::{ImageStore, Resources, Skin};
use crate::platform::Platform;
use crate::settings::{Lang, Settings};
use std::collections::HashMap;

/// 窗口移动走 IPC（macOS AX），没必要跟着 60fps 发
const IE_MOVE_INTERVAL: f64 = 1.0 / 30.0;
const POPULATION_CAP: usize = 50;

pub struct World {
    pub res: Resources,
    pub settings: Settings,
    pub lang: Lang,
    pub env: Environment,
    pub platform: Box<dyn Platform>,
    images: ImageStore,
    skins: HashMap<String, Skin>,
    behaviors: Vec<BehaviorDef>,
    index: HashMap<&'static str, usize>,
    pub mascots: Vec<Option<Box<Mascot>>>,
    pub added: Vec<Box<Mascot>>,
    next_id: u32,
    total: usize,
    frame: u64,
    clock: f64,
    last_ie_move: f64,
    pub debug: bool,
}

impl World {
    pub fn new(res: Resources, settings: Settings, platform: Box<dyn Platform>, debug: bool) -> Self {
        let lang = Lang::load(&res, &settings.language);
        let behaviors = behaviors::table();
        let index = behaviors.iter().enumerate().map(|(i, b)| (b.name, i)).collect();
        World {
            images: ImageStore::new(settings.opacity),
            res,
            settings,
            lang,
            env: Environment::new(),
            platform,
            skins: HashMap::new(),
            behaviors,
            index,
            mascots: Vec::new(),
            added: Vec::new(),
            next_id: 0,
            total: 0,
            frame: 0,
            clock: 0.0,
            last_ie_move: f64::NEG_INFINITY,
            debug,
        }
    }

    pub fn load_skin(&mut self, name: &str) -> anyhow::Result<()> {
        if !self.skins.contains_key(name) {
            let skin = Skin::load(&self.res, name, &self.images, self.settings.scaling)?;
            self.skins.insert(name.to_string(), skin);
        }
        Ok(())
    }

    pub fn skin(&self, name: &str) -> Option<&Skin> {
        self.skins.get(name)
    }

    pub fn has_loaded_skins(&self) -> bool {
        !self.skins.is_empty()
    }

    pub fn behaviors(&self) -> &[BehaviorDef] {
        &self.behaviors
    }

    pub fn behavior_enabled(&self, skin: &str, name: &str) -> bool {
        if REQUIRED.contains(&name) {
            return true;
        }
        let by_skin = self
            .skins
            .get(skin)
            .is_some_and(|s| s.manifest.behaviors.disabled.iter().any(|b| b == name));
        !by_skin && !self.settings.is_disabled(skin, name)
    }

    fn weight(&self, skin: &str, name: &str, default: u32) -> u32 {
        self.skins
            .get(skin)
            .and_then(|s| s.manifest.behaviors.weights.get(name).copied())
            .unwrap_or(default)
    }

    pub fn sense(&self, m: &mut Mascot) -> Sense {
        let p = m.p();
        let wa_id = self.env.work_area_for(&mut m.env, p, false);
        Sense {
            x: m.x,
            y: m.y,
            look_right: m.look_right,
            wa: self.env.area(wa_id).cloned().unwrap_or_default(),
            ie: self.env.active_ie.clone(),
            screen: self.env.screen.clone(),
            cursor: self.cursor(),
            cursor_v: (self.env.cursor.vx, self.env.cursor.vy),
            total: self.total.max(1),
        }
    }

    pub fn cursor(&self) -> (f64, f64) {
        (self.env.cursor.x, self.env.cursor.y)
    }

    pub fn throwing_enabled(&mut self) -> bool {
        self.settings.throwing && self.platform.can_move_windows()
    }

    /// 先在本地预测窗口新位置：扛窗口时下一帧的“是否还扛着”判定不必等系统回报
    pub fn move_ie(&mut self, x: i32, y: i32) {
        if self.clock - self.last_ie_move < IE_MOVE_INTERVAL {
            return;
        }
        self.last_ie_move = self.clock;
        let cur = self.env.active_ie.rect();
        let bounds = self.env.work_area.rect();
        self.platform.move_active_window(x, y, cur, bounds);
        self.env.active_ie.set(crate::sim::env::Rect { x, y, ..cur });
    }

    pub fn restore_ie(&mut self) {
        let bounds = self.env.work_area.rect();
        self.platform.restore_windows(bounds);
    }

    pub fn mascot(&self, id: u32) -> Option<&Mascot> {
        self.mascots
            .iter()
            .flatten()
            .find(|m| m.id == id && !m.disposed)
            .map(|b| b.as_ref())
    }

    pub fn with_mascot(&mut self, id: u32, f: impl FnOnce(&mut Mascot, &mut World)) {
        let Some(idx) = self.mascots.iter().position(|s| s.as_ref().is_some_and(|m| m.id == id)) else {
            return;
        };
        let Some(mut m) = self.mascots[idx].take() else { return };
        f(&mut m, self);
        self.mascots[idx] = Some(m);
    }

    pub fn live(&self) -> impl Iterator<Item = &Mascot> {
        self.mascots.iter().flatten().filter(|m| !m.disposed).map(|b| b.as_ref())
    }

    pub fn is_empty(&self) -> bool {
        self.live().next().is_none() && self.added.is_empty()
    }

    // ---------- 行为调度 ----------

    pub fn start(&self, m: &mut Mascot, name: &str) {
        let Some(&i) = self.index.get(name) else {
            eprintln!("[behavior] 未知行为 {name}");
            return;
        };
        let def = &self.behaviors[i];
        let s = self.sense(m);
        if self.debug {
            eprintln!("[mascot{}] {} → {}", m.id, m.behavior_name(), def.name);
        }
        m.brain = Some(Brain::Script(Script::new(def.name, (def.build)(&s))));
    }

    /// 被用户禁用的行为不执行，改为从天上掉下来（原版 buildBehavior(name, mascot) 语义）
    pub fn start_checked(&self, m: &mut Mascot, name: &str) {
        if self.behavior_enabled(&m.skin, name) {
            self.start(m, name);
        } else {
            self.respawn(m);
            self.start(m, "Fall");
        }
    }

    fn respawn(&self, m: &mut Mascot) {
        let area = if self.settings.multiscreen {
            self.env.screen.rect()
        } else {
            self.env.work_area.rect()
        };
        m.x = area.x as f64 + (rand::random::<f64>() * area.w.max(1) as f64).trunc();
        m.y = (area.y - 256) as f64;
    }

    fn pick_next(&self, prev: Option<&str>, m: &mut Mascot) -> Option<&'static str> {
        let s = self.sense(m);
        let skin = m.skin.clone();
        let mut candidates: Vec<(&'static str, u32)> = self
            .behaviors
            .iter()
            .map(|b| (b.name, self.weight(&skin, b.name, b.weight)))
            .filter(|&(name, w)| w > 0 && self.behavior_enabled(&skin, name))
            .filter(|&(name, _)| (self.behaviors[self.index[name]].when)(&s))
            .collect();
        if let Some(next) = prev.and_then(|p| self.index.get(p)).and_then(|&i| self.behaviors[i].next.as_ref()) {
            if !next.additive {
                candidates.clear();
            }
            for r in &next.refs {
                if r.weight > 0
                    && self.index.contains_key(r.name)
                    && self.behavior_enabled(&skin, r.name)
                    && r.when.is_none_or(|c| c(&s))
                {
                    candidates.push((r.name, r.weight));
                }
            }
        }
        let total: u64 = candidates.iter().map(|&(_, w)| w as u64).sum();
        if total == 0 {
            return None;
        }
        let mut roll = rand::random::<f64>() * total as f64;
        for (name, w) in candidates {
            roll -= w as f64;
            if roll < 0.0 {
                return Some(name);
            }
        }
        None
    }

    fn start_next(&self, prev: Option<&str>, m: &mut Mascot) {
        match self.pick_next(prev, m) {
            Some(name) => self.start(m, name),
            None => {
                self.respawn(m);
                self.start(m, "Fall");
            }
        }
    }

    fn update_mascot(&mut self, m: &mut Mascot, dt: f64) {
        if m.paused {
            return;
        }
        let Some(mut brain) = m.brain.take() else {
            self.start_next(None, m);
            return;
        };
        let name = Mascot::behavior_name_of(&brain);
        let outcome = match &mut brain {
            Brain::Script(s) => s.update(m, self, dt),
            Brain::Drag(d) => d.update(m, self, dt),
        };
        if m.brain.is_none() {
            m.brain = Some(brain);
        }
        match outcome {
            Outcome::Running => {
                let b = m.bounds();
                let screen = self.env.screen.rect();
                // 被扔出/走出屏幕：从屏幕顶上随机位置重新掉下来
                if b.right() <= screen.x || screen.right() <= b.x || screen.bottom() <= b.y {
                    self.respawn(m);
                    self.start(m, "Fall");
                }
            }
            Outcome::Done => self.start_next(Some(name), m),
            Outcome::Lost => self.start(m, "Fall"),
        }
    }

    pub fn tick(&mut self, dt: f64) {
        self.clock += dt;
        self.frame += 1;
        self.env.multiscreen = self.settings.multiscreen;
        self.env.tick(&mut *self.platform, dt, self.frame % 2 == 1);
        self.mascots.extend(self.added.drain(..).map(Some));
        self.mascots.retain(|s| s.as_ref().is_some_and(|m| !m.disposed));
        self.total = self.mascots.len();
        for i in 0..self.mascots.len() {
            let Some(mut m) = self.mascots[i].take() else { continue };
            self.update_mascot(&mut m, dt);
            self.mascots[i] = Some(m);
        }
    }

    /// 首次 tick 前先拿一次屏幕/光标，生成位置才有依据
    pub fn prime_env(&mut self) {
        self.env.tick(&mut *self.platform, 0.0, true);
    }

    // ---------- 繁殖 / 交互 ----------

    pub fn breed(&mut self, parent: &Mascot, born: (f64, f64), child_behavior: &str) {
        if self.total + self.added.len() >= POPULATION_CAP {
            return;
        }
        let scale = self.skin(&parent.skin).map_or(1.0, |s| s.scale);
        let (bx, by) = ((born.0 * scale).round(), (born.1 * scale).round());
        let mut child = Box::new(Mascot::new(self.alloc_id(), &parent.skin));
        child.x = if parent.look_right { parent.x - bx } else { parent.x + bx };
        child.y = parent.y + by;
        child.look_right = parent.look_right;
        self.start_checked(&mut child, child_behavior);
        self.added.push(child);
    }

    fn alloc_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    pub fn press(&mut self, id: u32) {
        let cx = self.env.cursor.x;
        self.with_mascot(id, |m, w| {
            if !m.paused {
                if w.debug {
                    eprintln!("[mascot{}] {} → Dragged", m.id, m.behavior_name());
                }
                m.brain = Some(Brain::Drag(Drag::new(cx)));
            }
        });
    }

    pub fn release(&mut self, id: u32) {
        self.with_mascot(id, |m, w| {
            if m.is_dragged() {
                w.start(m, "Thrown");
            }
        });
    }

    /// 新宠物放在屏幕外上方：第一次挑行为时什么都不满足 → 从天上掉下来
    pub fn spawn(&mut self, skin: &str) {
        if let Err(e) = self.load_skin(skin) {
            eprintln!("[skin] 加载 {skin} 失败: {e:#}");
            return;
        }
        let mut m = Box::new(Mascot::new(self.alloc_id(), skin));
        m.x = 10.0;
        m.y = -1000.0;
        m.look_right = rand::random::<f64>() < 0.5;
        self.start_next(None, &mut m);
        self.added.push(m);
    }

    pub fn spawn_random(&mut self) {
        let skins = self.active_skins();
        if skins.is_empty() {
            return;
        }
        let i = ((rand::random::<f64>() * skins.len() as f64) as usize).min(skins.len() - 1);
        let skin = skins[i].clone();
        self.spawn(&skin);
    }

    pub fn active_skins(&self) -> Vec<String> {
        let all = self.res.list_skins();
        let active: Vec<String> = self
            .settings
            .active_skins
            .iter()
            .filter(|s| all.contains(s))
            .cloned()
            .collect();
        if !active.is_empty() {
            return active;
        }
        all.iter()
            .find(|s| s.as_str() == "Shimeji")
            .or(all.first())
            .cloned()
            .into_iter()
            .collect()
    }

    fn for_each_mascot(&mut self, mut f: impl FnMut(&mut Mascot, &mut World)) {
        let ids: Vec<u32> = self.live().map(|m| m.id).collect();
        for id in ids {
            self.with_mascot(id, &mut f);
        }
    }

    pub fn set_behavior_all(&mut self, name: &str, skin: Option<&str>) {
        self.for_each_mascot(|m, w| {
            if skin.is_none_or(|s| s == m.skin) {
                w.start_checked(m, name);
            }
        });
    }

    pub fn set_behavior_of(&mut self, id: u32, name: &str) {
        self.with_mascot(id, |m, w| w.start_checked(m, name));
    }

    pub fn dispose(&mut self, id: u32) {
        self.with_mascot(id, |m, _| m.disposed = true);
    }

    pub fn dispose_all(&mut self) {
        self.dispose_except(|_| false);
        self.added.clear();
    }

    pub fn dispose_skin(&mut self, skin: &str) {
        self.dispose_except(|m| m.skin != skin);
    }

    /// 托盘「仅剩一个」：保留最早的那只
    pub fn remain_one(&mut self) {
        let keep = self.live().next().map(|m| m.id);
        self.dispose_except(|m| Some(m.id) == keep);
    }

    pub fn remain_only(&mut self, id: u32) {
        self.dispose_except(|m| m.id == id);
    }

    /// 右键「仅剩一个」：同皮肤只留这一只
    pub fn remain_one_of_skin(&mut self, id: u32) {
        let Some(skin) = self.mascot(id).map(|m| m.skin.clone()) else { return };
        self.dispose_except(|m| m.skin != skin || m.id == id);
    }

    fn dispose_except(&mut self, keep: impl Fn(&Mascot) -> bool) {
        for m in self.mascots.iter_mut().flatten() {
            if !keep(m) {
                m.disposed = true;
            }
        }
    }

    pub fn all_paused(&self) -> bool {
        self.live().all(|m| m.paused)
    }

    pub fn toggle_pause_all(&mut self) {
        let pause = !self.all_paused();
        for m in self.mascots.iter_mut().flatten() {
            m.paused = pause;
        }
    }

    pub fn toggle_pause(&mut self, id: u32) {
        self.with_mascot(id, |m, _| m.paused = !m.paused);
    }
}
