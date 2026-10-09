use super::clips::Pose;
use super::engine::{Drag, Script};
use super::env::{MascotEnv, Rect};
use crate::assets::Sprite;
use super::world::World;
use std::rc::Rc;

/// 当前帧图像。原图朝左，朝右时水平镜像，锚点也跟着镜像
#[derive(Clone)]
pub struct FrameImage {
    pub sprite: Rc<Sprite>,
    pub flipped: bool,
    pub center: (i32, i32),
}

impl FrameImage {
    pub fn size(&self) -> (i32, i32) {
        (self.sprite.width as i32, self.sprite.height as i32)
    }

    pub fn alpha_at(&self, x: i32, y: i32) -> u8 {
        let x = if self.flipped { self.sprite.width as i32 - 1 - x } else { x };
        self.sprite.alpha_at(x, y)
    }
}

pub enum Brain {
    Script(Script),
    Drag(Drag),
}

pub struct Mascot {
    pub id: u32,
    pub skin: String,
    /// 脚底坐标（逻辑像素，亚像素精度）。挂在墙上/天花板时是手的位置
    pub x: f64,
    pub y: f64,
    pub look_right: bool,
    pub image: Option<FrameImage>,
    pub brain: Option<Brain>,
    pub paused: bool,
    pub disposed: bool,
    pub env: MascotEnv,
    last_bounds: Rect,
}

impl Mascot {
    pub fn new(id: u32, skin: &str) -> Self {
        Mascot {
            id,
            skin: skin.to_string(),
            x: 0.0,
            y: 0.0,
            look_right: false,
            image: None,
            brain: None,
            paused: false,
            disposed: false,
            env: MascotEnv::default(),
            last_bounds: Rect::default(),
        }
    }

    /// 贴边判定与窗口定位用的整数坐标
    pub fn p(&self) -> (i32, i32) {
        (self.x.round() as i32, self.y.round() as i32)
    }

    /// 剪辑里的速度以原图（朝左）为准
    pub fn move_pose(&mut self, dx: f64, dy: f64) {
        self.x += if self.look_right { -dx } else { dx };
        self.y += dy;
    }

    pub fn show(&mut self, w: &World, pose: &Pose) {
        self.show_frame(w, pose.frame, pose.anchor);
    }

    pub fn show_frame(&mut self, w: &World, frame: u8, clip_anchor: Option<(i32, i32)>) {
        let Some(skin) = w.skin(&self.skin) else { return };
        let sprite = skin.frame(frame).clone();
        let (ax, ay) = skin.anchor(frame, clip_anchor);
        let center = if self.look_right { (sprite.width as i32 - ax, ay) } else { (ax, ay) };
        self.image = Some(FrameImage {
            sprite,
            flipped: self.look_right,
            center,
        });
    }

    pub fn image_bounds(&self) -> Option<Rect> {
        let (px, py) = self.p();
        self.image.as_ref().map(|img| {
            let (w, h) = img.size();
            Rect {
                x: px - img.center.0,
                y: py - img.center.1,
                w,
                h,
            }
        })
    }

    /// 无图时沿用上一帧的边界
    pub fn bounds(&mut self) -> Rect {
        if let Some(b) = self.image_bounds() {
            self.last_bounds = b;
        }
        self.last_bounds
    }

    pub fn is_dragged(&self) -> bool {
        matches!(self.brain, Some(Brain::Drag(_)))
    }

    pub fn behavior_name(&self) -> &'static str {
        self.brain.as_ref().map_or("", Self::behavior_name_of)
    }

    pub fn behavior_name_of(brain: &Brain) -> &'static str {
        match brain {
            Brain::Script(s) => s.name,
            Brain::Drag(_) => "Dragged",
        }
    }
}
