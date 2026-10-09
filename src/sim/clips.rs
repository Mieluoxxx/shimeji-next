//! 标准 46 帧的动画剪辑。数据取自 Shimeji-ee 默认 actions.xml，
//! 原版以 40ms tick 为单位（Duration=ticks，Velocity=像素/tick），这里统一换算成秒与像素/秒，
//! 引擎按真实 dt 推进，帧率无关。

/// 原版 1 tick
const TICK: f64 = 0.04;

#[derive(Debug, Clone, Copy)]
pub struct Pose {
    pub frame: u8,
    pub secs: f64,
    /// 像素/秒，以“朝左”为正方向的原图坐标系（朝右时 x 取反）
    pub vx: f64,
    pub vy: f64,
    pub anchor: Option<(i32, i32)>,
}

const fn p(frame: u8, ticks: u32, vx: f64, vy: f64) -> Pose {
    Pose {
        frame,
        secs: ticks as f64 * TICK,
        vx: vx / TICK,
        vy: vy / TICK,
        anchor: None,
    }
}

const fn pa(frame: u8, ticks: u32, vx: f64, vy: f64, ax: i32, ay: i32) -> Pose {
    Pose {
        anchor: Some((ax, ay)),
        ..p(frame, ticks, vx, vy)
    }
}

pub struct Clip {
    pub poses: &'static [Pose],
}

impl Clip {
    pub fn duration(&self) -> f64 {
        self.poses.iter().map(|p| p.secs).sum()
    }

    pub fn pose_at(&self, t: f64) -> &Pose {
        let total = self.duration();
        let mut t = if total > 0.0 { t.rem_euclid(total) } else { 0.0 };
        for pose in self.poses {
            t -= pose.secs;
            if t < 0.0 {
                return pose;
            }
        }
        self.poses.last().expect("剪辑非空")
    }
}

/// 帧的默认锚点（脚底在 128x128 原图中的位置）
pub fn default_anchor(frame: u8) -> (i32, i32) {
    match frame {
        23..=25 => (64, 48),
        30..=33 => (64, 112),
        _ => (64, 128),
    }
}

macro_rules! clip {
    ($name:ident = [$($pose:expr),+ $(,)?]) => {
        pub static $name: Clip = Clip { poses: &[$($pose),+] };
    };
}

clip!(STAND = [p(1, 250, 0.0, 0.0)]);
clip!(WALK = [p(1, 6, -2.0, 0.0), p(2, 6, -2.0, 0.0), p(1, 6, -2.0, 0.0), p(3, 6, -2.0, 0.0)]);
clip!(RUN = [p(1, 2, -4.0, 0.0), p(2, 2, -4.0, 0.0), p(1, 2, -4.0, 0.0), p(3, 2, -4.0, 0.0)]);
clip!(DASH = [p(1, 2, -8.0, 0.0), p(2, 2, -8.0, 0.0), p(1, 2, -8.0, 0.0), p(3, 2, -8.0, 0.0)]);

clip!(SIT = [p(11, 250, 0.0, 0.0)]);
clip!(SIT_LOOK_UP = [p(26, 250, 0.0, 0.0)]);
clip!(SIT_SPIN_HEAD = [
    p(26, 5, 0.0, 0.0), p(15, 5, 0.0, 0.0), p(27, 5, 0.0, 0.0), p(16, 5, 0.0, 0.0),
    p(28, 5, 0.0, 0.0), p(17, 5, 0.0, 0.0), p(29, 5, 0.0, 0.0), p(11, 5, 0.0, 0.0),
]);
clip!(SIT_LEGS_UP = [p(30, 250, 0.0, 0.0)]);
clip!(SIT_LEGS_DOWN = [p(31, 250, 0.0, 0.0)]);
clip!(SIT_DANGLE_LEGS = [p(31, 5, 0.0, 0.0), p(32, 15, 0.0, 0.0), p(31, 5, 0.0, 0.0), p(33, 15, 0.0, 0.0)]);

clip!(SPRAWL = [p(21, 250, 0.0, 0.0)]);
clip!(CREEP = [
    p(20, 28, 0.0, 0.0), p(20, 4, -2.0, 0.0), p(21, 4, -2.0, 0.0), p(21, 4, -1.0, 0.0), p(21, 24, 0.0, 0.0),
]);

clip!(GRAB_CEILING = [p(23, 250, 0.0, 0.0)]);
clip!(CLIMB_CEILING = [
    p(25, 16, 0.0, 0.0), p(25, 4, -1.0, 0.0), p(23, 4, -1.0, 0.0), p(24, 4, -1.0, 0.0),
    p(24, 16, 0.0, 0.0), p(24, 4, -2.0, 0.0), p(23, 4, -2.0, 0.0), p(25, 4, -2.0, 0.0),
]);

clip!(GRAB_WALL = [p(13, 250, 0.0, 0.0)]);
clip!(CLIMB_WALL_UP = [
    p(14, 16, 0.0, 0.0), p(14, 4, 0.0, -1.0), p(12, 4, 0.0, -1.0), p(13, 4, 0.0, -1.0),
    p(13, 16, 0.0, 0.0), p(13, 4, 0.0, -2.0), p(12, 4, 0.0, -2.0), p(14, 4, 0.0, -2.0),
]);
clip!(CLIMB_WALL_DOWN = [
    p(14, 16, 0.0, 0.0), p(14, 4, 0.0, 2.0), p(12, 4, 0.0, 2.0), p(13, 4, 0.0, 2.0),
    p(13, 16, 0.0, 0.0), p(13, 4, 0.0, 1.0), p(12, 4, 0.0, 1.0), p(14, 4, 0.0, 1.0),
]);

clip!(HOLD_WINDOW = [p(36, 250, 0.0, 0.0)]);
clip!(WALK_WITH_WINDOW = [p(34, 6, -2.0, 0.0), p(35, 6, -2.0, 0.0), p(34, 6, -2.0, 0.0), p(36, 6, -2.0, 0.0)]);
clip!(RUN_WITH_WINDOW = [p(34, 2, -8.0, 0.0), p(35, 2, -8.0, 0.0), p(34, 2, -8.0, 0.0), p(36, 2, -8.0, 0.0)]);
clip!(THROW_WINDOW = [p(37, 40, 0.0, 0.0)]);

clip!(JUMPING = [p(22, 250, 0.0, 0.0)]);
clip!(FALLING = [p(4, 250, 0.0, 0.0)]);
clip!(BOUNCING = [p(18, 4, 0.0, 0.0), p(19, 4, 0.0, 0.0)]);
clip!(TRIPPING = [
    p(19, 8, -8.0, 0.0), p(18, 4, -4.0, 0.0), p(20, 4, -2.0, 0.0), p(20, 10, 0.0, 0.0),
    pa(19, 4, -4.0, 0.0, 64, 104),
]);

clip!(RESISTING = [
    p(5, 5, 0.0, 0.0), p(6, 5, 0.0, 0.0), p(5, 5, 0.0, 0.0), p(6, 5, 0.0, 0.0),
    p(1, 50, 0.0, 0.0),
    p(5, 5, 0.0, 0.0), p(6, 5, 0.0, 0.0), p(5, 5, 0.0, 0.0), p(6, 5, 0.0, 0.0),
    p(5, 5, 0.0, 0.0), p(6, 5, 0.0, 0.0), p(5, 5, 0.0, 0.0), p(6, 5, 0.0, 0.0),
    p(1, 100, 0.0, 0.0),
    p(5, 5, 0.0, 0.0), p(6, 5, 0.0, 0.0), p(5, 5, 0.0, 0.0), p(6, 5, 0.0, 0.0),
    p(5, 5, 0.0, 0.0), p(6, 5, 0.0, 0.0), p(5, 5, 0.0, 0.0), p(6, 5, 0.0, 0.0),
    p(5, 2, 0.0, 0.0), p(6, 2, 0.0, 0.0), p(5, 2, 0.0, 0.0), p(6, 2, 0.0, 0.0),
    p(5, 2, 0.0, 0.0), p(6, 2, 0.0, 0.0), p(5, 2, 0.0, 0.0), p(6, 2, 0.0, 0.0),
]);

clip!(PULL_UP = [
    p(1, 16, 0.0, 0.0),
    pa(38, 4, 0.0, 0.0, 96, 128),
    pa(39, 40, 0.0, 0.0, 96, 128),
    pa(40, 40, 0.0, 0.0, 96, 128),
    pa(41, 40, 0.0, 0.0, 96, 128),
]);
clip!(PULL_UP_RECOIL = [
    pa(9, 1, 20.0, -20.0, 32, 128),
    pa(9, 1, 20.0, -10.0, 32, 128),
    pa(9, 1, 20.0, -5.0, 32, 128),
]);
clip!(DIVIDE = [p(42, 5, 0.0, 0.0), p(43, 2, 0.0, 0.0), p(44, 2, 0.0, 0.0), p(45, 5, 0.0, 0.0), p(46, 20, 0.0, 0.0)]);

/// 被拎着时按“脚相对光标的偏移”选的帧（原版 Pinched 的 7 个条件分支）
pub fn pinched_frame(foot_x: f64, cursor_x: f64) -> u8 {
    let c = cursor_x;
    if foot_x < c - 50.0 {
        9
    } else if foot_x < c - 30.0 {
        7
    } else if foot_x < c {
        5
    } else if foot_x > c - 10.0 && foot_x < c + 10.0 {
        1
    } else if foot_x < c + 30.0 {
        6
    } else if foot_x < c + 50.0 {
        8
    } else {
        10
    }
}

/// 原版 tick 数 → 秒，给 behaviors.rs 写随机时长用
pub const fn ticks(n: f64) -> f64 {
    n * TICK
}

/// 原版 像素/tick → 像素/秒
pub const fn per_tick(v: f64) -> f64 {
    v / TICK
}
