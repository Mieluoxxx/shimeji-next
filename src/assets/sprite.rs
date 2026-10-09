//! 皮肤帧的 CPU 侧缓存：GPU 上传与逐像素命中测试共用同一份 RGBA。

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub struct Sprite {
    /// 渲染层纹理缓存的键
    pub key: String,
    pub width: u32,
    pub height: u32,
    /// straight alpha（非预乘）：Metal 合成只接受 PostMultiplied
    pub rgba: Vec<u8>,
}

impl Sprite {
    pub fn alpha_at(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return 0;
        }
        self.rgba[(y as usize * self.width as usize + x as usize) * 4 + 3]
    }
}

pub struct ImageStore {
    cache: RefCell<HashMap<(PathBuf, u64), Rc<Sprite>>>,
    opacity: f64,
}

impl ImageStore {
    pub fn new(opacity: f64) -> Self {
        ImageStore {
            cache: RefCell::default(),
            opacity,
        }
    }

    pub fn load(&self, path: &Path, scale: f64) -> anyhow::Result<Rc<Sprite>> {
        let key = (path.to_path_buf(), scale.to_bits());
        if let Some(s) = self.cache.borrow().get(&key) {
            return Ok(s.clone());
        }
        let mut img = image::open(path)
            .map_err(|e| anyhow::anyhow!("加载图像失败 {}: {e}", path.display()))?
            .to_rgba8();
        if (scale - 1.0).abs() > f64::EPSILON {
            let w = ((img.width() as f64) * scale).round().max(1.0) as u32;
            let h = ((img.height() as f64) * scale).round().max(1.0) as u32;
            img = image::imageops::resize(&img, w, h, image::imageops::FilterType::Nearest);
        }
        if self.opacity < 1.0 {
            for px in img.pixels_mut() {
                px.0[3] = (px.0[3] as f64 * self.opacity.max(0.0)).round() as u8;
            }
        }
        let sprite = Rc::new(Sprite {
            key: format!("{}@{scale}", path.display()),
            width: img.width(),
            height: img.height(),
            rgba: img.into_raw(),
        });
        self.cache.borrow_mut().insert(key, sprite.clone());
        Ok(sprite)
    }
}
