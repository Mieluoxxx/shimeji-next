//! 用户设置（settings.toml）与界面文案（i18n/<语言>.toml）。

use crate::assets::{user_data_dir, Resources};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub active_skins: Vec<String>,
    /// i18n 文件名，如 zh、en、ja、zh-TW
    pub language: String,
    pub breeding: bool,
    /// 扛起/扔窗口；macOS 上还需要辅助功能授权
    pub throwing: bool,
    pub multiscreen: bool,
    pub scaling: f64,
    pub opacity: f64,
    /// 皮肤名 → 被禁用的行为
    pub disabled_behaviors: BTreeMap<String, Vec<String>>,
    #[serde(skip)]
    path: PathBuf,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            active_skins: vec!["Shimeji".into()],
            language: "zh".into(),
            breeding: true,
            throwing: true,
            multiscreen: true,
            scaling: 1.0,
            opacity: 1.0,
            disabled_behaviors: BTreeMap::new(),
            path: PathBuf::new(),
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let path = user_data_dir().join("settings.toml");
        let mut s: Settings = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                eprintln!("[settings] {} 解析失败，使用默认值: {e}", path.display());
                Settings::default()
            }),
            Err(_) => Settings::default(),
        };
        s.scaling = s.scaling.clamp(0.25, 8.0);
        s.opacity = s.opacity.clamp(0.05, 1.0);
        s.path = path;
        s
    }

    /// 测试/临时运行用，不落盘
    pub fn ephemeral() -> Self {
        Settings::default()
    }

    pub fn save(&self) {
        if self.path.as_os_str().is_empty() {
            return;
        }
        let result = toml::to_string_pretty(self)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                if let Some(dir) = self.path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                std::fs::write(&self.path, text).map_err(|e| e.to_string())
            });
        if let Err(e) = result {
            eprintln!("[settings] 保存失败 {}: {e}", self.path.display());
        }
    }

    pub fn is_disabled(&self, skin: &str, behavior: &str) -> bool {
        self.disabled_behaviors
            .get(skin)
            .is_some_and(|l| l.iter().any(|b| b == behavior))
    }

    pub fn toggle_behavior(&mut self, skin: &str, behavior: &str) {
        let list = self.disabled_behaviors.entry(skin.to_string()).or_default();
        match list.iter().position(|b| b == behavior) {
            Some(i) => {
                list.remove(i);
            }
            None => list.push(behavior.to_string()),
        }
        self.disabled_behaviors.retain(|_, l| !l.is_empty());
        self.save();
    }
}

pub struct Lang {
    texts: HashMap<String, String>,
}

impl Lang {
    /// 指定语言缺的键回退 en；"zh-CN" 这类找不到时退到主语言 "zh"
    pub fn load(res: &Resources, language: &str) -> Self {
        let dir = res.root.join("i18n");
        let read = |code: &str| -> HashMap<String, String> {
            std::fs::read_to_string(dir.join(format!("{code}.toml")))
                .ok()
                .and_then(|t| toml::from_str(&t).ok())
                .unwrap_or_default()
        };
        let mut texts = read("en");
        let primary = language.split(['-', '_']).next().unwrap_or("en");
        let specific = read(language);
        texts.extend(if specific.is_empty() { read(primary) } else { specific });
        Lang { texts }
    }

    pub fn tr(&self, key: &str) -> String {
        self.texts.get(key).cloned().unwrap_or_else(|| key.to_string())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.texts.get(key).map(String::as_str)
    }
}
