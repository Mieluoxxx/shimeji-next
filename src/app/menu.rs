//! 托盘菜单与宠物右键菜单。菜单项 id 是字符串，回调里解析回 Command。

use crate::sim::behaviors::REQUIRED;
use crate::sim::World;
use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

#[derive(Debug, Clone, Copy)]
pub enum Flag {
    Breeding,
    Throwing,
    Multiscreen,
}

impl Flag {
    const ALL: [(Flag, &'static str, &'static str); 3] = [
        (Flag::Breeding, "breeding", "BreedingCloning"),
        (Flag::Throwing, "throwing", "ThrowingWindows"),
        (Flag::Multiscreen, "multiscreen", "Multiscreen"),
    ];

    fn get(self, w: &World) -> bool {
        match self {
            Flag::Breeding => w.settings.breeding,
            Flag::Throwing => w.settings.throwing,
            Flag::Multiscreen => w.settings.multiscreen,
        }
    }
}

pub enum MascotOp {
    CallAnother,
    Follow,
    Restore,
    SetBehavior(String),
    ToggleBehavior(String),
    Pause,
    Dismiss,
    DismissOthers,
    DismissAllOthers,
}

pub enum Command {
    Call,
    Follow,
    ReduceToOne,
    Restore,
    PauseAll,
    OpenSkins,
    Rescan,
    Quit,
    Toggle(Flag),
    Skin(String),
    Mascot(u32, MascotOp),
}

impl Command {
    pub fn parse(id: &str) -> Option<Command> {
        let (head, rest) = id.split_once(':')?;
        Some(match head {
            "tray" => match rest {
                "call" => Command::Call,
                "follow" => Command::Follow,
                "one" => Command::ReduceToOne,
                "restore" => Command::Restore,
                "pause" => Command::PauseAll,
                "skins" => Command::OpenSkins,
                "rescan" => Command::Rescan,
                "quit" => Command::Quit,
                _ => return None,
            },
            "flag" => Command::Toggle(Flag::ALL.iter().find(|(_, k, _)| *k == rest)?.0),
            "skin" => Command::Skin(rest.to_string()),
            "m" => {
                let (id, op) = rest.split_once(':')?;
                let id: u32 = id.parse().ok()?;
                let (op, arg) = op.split_once(':').unwrap_or((op, ""));
                let op = match op {
                    "call" => MascotOp::CallAnother,
                    "follow" => MascotOp::Follow,
                    "restore" => MascotOp::Restore,
                    "beh" => MascotOp::SetBehavior(arg.to_string()),
                    "tog" => MascotOp::ToggleBehavior(arg.to_string()),
                    "pause" => MascotOp::Pause,
                    "dismiss" => MascotOp::Dismiss,
                    "others" => MascotOp::DismissOthers,
                    "allothers" => MascotOp::DismissAllOthers,
                    _ => return None,
                };
                Command::Mascot(id, op)
            }
            _ => return None,
        })
    }
}

/// 没有翻译的行为名按驼峰拆词，IE 单独成词（"WalkAlongIECeiling" → "Walk Along IE Ceiling"）
pub fn caption(name: &str) -> String {
    let c: Vec<char> = name.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        out.push(c[i]);
        if c[i].is_ascii_lowercase() {
            if i + 3 < c.len() && c[i + 1] == 'I' && c[i + 2] == 'E' && c[i + 3].is_ascii_uppercase() {
                out.push_str(" IE ");
                i += 3;
                continue;
            }
            if i + 1 < c.len() && c[i + 1].is_ascii_uppercase() {
                out.push(' ');
            }
        }
        i += 1;
    }
    out
}

fn item(id: &str, text: &str) -> MenuItem {
    MenuItem::with_id(id, text, true, None)
}

fn sep() -> PredefinedMenuItem {
    PredefinedMenuItem::separator()
}

fn append_all(menu: &Menu, items: &[&dyn IsMenuItem]) {
    if let Err(e) = menu.append_items(items) {
        eprintln!("[menu] {e}");
    }
}

fn pause_label(w: &World) -> String {
    w.lang.tr(if w.all_paused() { "ResumeAnimations" } else { "PauseAnimations" })
}

pub struct Tray {
    _icon: TrayIcon,
    pause: MenuItem,
    flags: Vec<(Flag, CheckMenuItem)>,
    skins: Vec<(String, CheckMenuItem)>,
}

impl Tray {
    pub fn new(w: &World) -> anyhow::Result<Tray> {
        let l = &w.lang;
        let menu = Menu::new();
        let pause = item("tray:pause", &pause_label(w));

        let allowed = Submenu::new(l.tr("AllowedBehaviours"), true);
        let mut flags = Vec::new();
        for (flag, key, label) in Flag::ALL {
            let check = CheckMenuItem::with_id(format!("flag:{key}"), l.tr(label), true, flag.get(w), None);
            allowed.append(&check)?;
            flags.push((flag, check));
        }

        let choose = Submenu::new(l.tr("ChooseShimeji"), true);
        let active = w.active_skins();
        let mut skins = Vec::new();
        for skin in w.res.list_skins() {
            let label = w.res.display_name(&skin);
            let check = CheckMenuItem::with_id(format!("skin:{skin}"), &label, true, active.contains(&skin), None);
            choose.append(&check)?;
            skins.push((skin, check));
        }
        choose.append_items(&[
            &sep(),
            &item("tray:skins", &l.tr("OpenSkinFolder")),
            &item("tray:rescan", &l.tr("RescanSkins")),
        ])?;

        append_all(
            &menu,
            &[
                &item("tray:call", &l.tr("CallShimeji")),
                &item("tray:follow", &l.tr("FollowCursor")),
                &item("tray:one", &l.tr("ReduceToOne")),
                &item("tray:restore", &l.tr("RestoreWindows")),
                &sep(),
                &allowed,
                &choose,
                &pause,
                &sep(),
                &item("tray:quit", &l.tr("DismissAll")),
            ],
        );

        let icon_path = w.res.root.join("icon.png");
        let icon = image::open(&icon_path)
            .map_err(|e| anyhow::anyhow!("读取托盘图标 {} 失败: {e}", icon_path.display()))?
            .to_rgba8();
        let (iw, ih) = icon.dimensions();
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(Icon::from_rgba(icon.into_raw(), iw, ih)?)
            .with_tooltip("Shimeji Next")
            .build()?;
        Ok(Tray { _icon: tray, pause, flags, skins })
    }

    pub fn refresh(&self, w: &World) {
        self.pause.set_text(pause_label(w));
        for (flag, check) in &self.flags {
            check.set_checked(flag.get(w));
        }
        let active = w.active_skins();
        for (skin, check) in &self.skins {
            check.set_checked(active.contains(skin));
        }
    }
}

pub fn mascot_menu(w: &World, id: u32) -> Option<Menu> {
    let m = w.mascot(id)?;
    let l = &w.lang;
    let p = |op: &str| format!("m:{id}:{op}");

    let set_behavior = Submenu::new(l.tr("SetBehaviour"), true);
    let allowed = Submenu::new(l.tr("AllowedBehaviours"), true);
    for def in w.behaviors().iter().filter(|d| !d.hidden) {
        let text = l.get(def.name).map(str::to_string).unwrap_or_else(|| caption(def.name));
        let enabled = w.behavior_enabled(&m.skin, def.name);
        if enabled {
            let _ = set_behavior.append(&item(&p(&format!("beh:{}", def.name)), &text));
        }
        if !REQUIRED.contains(&def.name) {
            let check = CheckMenuItem::with_id(p(&format!("tog:{}", def.name)), &text, true, enabled, None);
            let _ = allowed.append(&check);
        }
    }

    let pause = if m.paused { l.tr("ResumeAnimations") } else { l.tr("PauseAnimations") };
    let menu = Menu::new();
    append_all(
        &menu,
        &[
            &item(&p("call"), &l.tr("CallAnother")),
            &sep(),
            &item(&p("follow"), &l.tr("FollowCursor")),
            &item(&p("restore"), &l.tr("RestoreWindows")),
            &sep(),
            &set_behavior,
            &allowed,
            &sep(),
            &item(&p("pause"), &pause),
            &sep(),
            &item(&p("dismiss"), &l.tr("Dismiss")),
            &item(&p("others"), &l.tr("DismissOthers")),
            &item(&p("allothers"), &l.tr("DismissAllOthers")),
            &item("tray:quit", &l.tr("DismissAll")),
        ],
    );
    Some(menu)
}

#[cfg(test)]
mod tests {
    use super::caption;

    #[test]
    fn captions_split_camel_case() {
        assert_eq!(caption("WalkAlongIECeiling"), "Walk Along IE Ceiling");
        assert_eq!(caption("SitDown"), "Sit Down");
        assert_eq!(caption("ThrowIEFromLeft"), "Throw IE From Left");
    }
}
