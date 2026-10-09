//! 资源定位与皮肤加载。
//!
//! 皮肤 = 一个文件夹：shime1.png ~ shime46.png（Shimeji 社区通行的帧约定）+ 可选 skin.toml。
//! 帧的语义（几号是站、几号是爬墙…）由 sim::clips 固定，皮肤只提供图。

mod sprite;

pub use sprite::{ImageStore, Sprite};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub const FRAME_COUNT: usize = 46;

/// 用户可写的数据目录（设置、用户皮肤）。.app 包内资源只读，用户改动都落在这里
pub fn user_data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/shimeji-next")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
            .join("shimeji-next")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("shimeji-next")
    }
}

/// 皮肤来源两处：随程序分发的内置皮肤（只读）+ 用户皮肤目录；同名时用户的优先，便于覆盖内置皮肤
#[derive(Debug, Clone)]
pub struct Resources {
    pub root: PathBuf,
    pub skins: PathBuf,
    pub user_skins: Option<PathBuf>,
}

impl Resources {
    pub fn new(root: &Path) -> Self {
        Resources {
            root: root.to_path_buf(),
            skins: root.join("skins"),
            user_skins: None,
        }
    }

    pub fn with_user_skins(mut self, dir: PathBuf) -> Self {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("[skins] 无法创建用户皮肤目录 {}: {e}", dir.display());
        }
        self.user_skins = Some(dir);
        self
    }

    pub fn is_valid(root: &Path) -> bool {
        root.join("i18n").join("en.toml").is_file() && root.join("skins").is_dir()
    }

    fn is_skin(dir: &Path) -> bool {
        dir.join("shime1.png").is_file()
    }

    pub fn skin_dir(&self, name: &str) -> PathBuf {
        if let Some(user) = &self.user_skins {
            let d = user.join(name);
            if Self::is_skin(&d) {
                return d;
            }
        }
        self.skins.join(name)
    }

    pub fn has_skin(&self, name: &str) -> bool {
        !name.is_empty() && Self::is_skin(&self.skin_dir(name))
    }

    /// 菜单里显示的名字：skin.toml 的 name，没有就用文件夹名。只读清单，不加载帧图
    pub fn display_name(&self, name: &str) -> String {
        Manifest::load(&self.skin_dir(name))
            .ok()
            .and_then(|m| m.name)
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| name.to_string())
    }

    pub fn list_skins(&self) -> Vec<String> {
        let mut skins: Vec<String> = [Some(&self.skins), self.user_skins.as_ref()]
            .into_iter()
            .flatten()
            .flat_map(|root| std::fs::read_dir(root).into_iter().flatten().flatten())
            .filter(|e| Self::is_skin(&e.path()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n != "unused" && !n.starts_with('.'))
            .collect();
        skins.sort();
        skins.dedup();
        skins
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Manifest {
    pub name: Option<String>,
    pub author: Option<String>,
    pub license: Option<String>,
    /// 叠加在全局 Scaling 之上
    pub scale: Option<f64>,
    /// 移动类动作（走/跑/爬）的速度倍率，播放节奏同步加快，脚步不打滑
    pub speed: Option<f64>,
    /// 帧号 → 锚点（脚底在原图中的像素坐标），覆盖内置默认
    pub anchors: HashMap<String, [i32; 2]>,
    pub behaviors: BehaviorTweaks,
}

impl Manifest {
    /// 没有 skin.toml 视为空清单；有但写错了报错，免得配置被静默忽略
    pub fn load(dir: &Path) -> anyhow::Result<Manifest> {
        let path = dir.join("skin.toml");
        match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|e| anyhow::anyhow!("{} 格式错误: {e}", path.display())),
            Err(_) => Ok(Manifest::default()),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BehaviorTweaks {
    pub disabled: Vec<String>,
    pub weights: HashMap<String, u32>,
}

pub struct Skin {
    pub name: String,
    pub manifest: Manifest,
    pub scale: f64,
    pub speed: f64,
    frames: Vec<Option<Rc<Sprite>>>,
    anchors: HashMap<u8, (i32, i32)>,
}

impl Skin {
    pub fn load(res: &Resources, name: &str, images: &ImageStore, global_scale: f64) -> anyhow::Result<Skin> {
        let dir = res.skin_dir(name);
        let manifest = Manifest::load(&dir)?;
        let scale = (global_scale * manifest.scale.unwrap_or(1.0)).clamp(0.25, 8.0);
        let mut frames = vec![None; FRAME_COUNT + 1];
        for (i, slot) in frames.iter_mut().enumerate().skip(1) {
            let path = dir.join(format!("shime{i}.png"));
            if path.is_file() {
                match images.load(&path, scale) {
                    Ok(s) => *slot = Some(s),
                    Err(e) => eprintln!("[skin] {name}: {e:#}"),
                }
            }
        }
        anyhow::ensure!(frames[1].is_some(), "{name} 缺少 shime1.png");
        let anchors = manifest
            .anchors
            .iter()
            .filter_map(|(k, v)| Some((k.trim().parse::<u8>().ok()?, (v[0], v[1]))))
            .collect();
        Ok(Skin {
            name: name.to_string(),
            speed: manifest.speed.unwrap_or(1.0).clamp(0.1, 10.0),
            manifest,
            scale,
            frames,
            anchors,
        })
    }

    /// 缺帧时用 1 号（站立）顶替，宁可动作不对也不要隐身
    pub fn frame(&self, n: u8) -> &Rc<Sprite> {
        self.frames
            .get(n as usize)
            .and_then(Option::as_ref)
            .or(self.frames[1].as_ref())
            .expect("shime1 已在加载时校验")
    }

    /// 锚点优先级：skin.toml > 动画里的特写锚点 > 帧的默认锚点；结果已按缩放换算
    pub fn anchor(&self, n: u8, clip_override: Option<(i32, i32)>) -> (i32, i32) {
        let raw = self
            .anchors
            .get(&n)
            .copied()
            .or(clip_override)
            .unwrap_or_else(|| crate::sim::clips::default_anchor(n));
        (
            (raw.0 as f64 * self.scale).round() as i32,
            (raw.1 as f64 * self.scale).round() as i32,
        )
    }

    pub fn display_name(&self) -> &str {
        self.manifest.name.as_deref().unwrap_or(&self.name)
    }
}
