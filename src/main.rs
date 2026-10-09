//! 用法：shimeji-next [--assets <资源目录>] [--skin <皮肤名>]... [--debug]
//! 资源目录含 i18n/ 与 skins/；开发时默认用项目内的 assets/。

// 发布版的 Windows 程序不要附带控制台窗口
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use anyhow::Context as _;
use shimeji_next::assets::{user_data_dir, Resources};
use shimeji_next::settings::Settings;
use shimeji_next::sim::World;
use std::path::PathBuf;

struct Args {
    assets: Option<PathBuf>,
    skins: Vec<String>,
    debug: bool,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut args = Args {
        assets: None,
        skins: Vec::new(),
        debug: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--assets" => args.assets = Some(it.next().context("--assets 需要一个目录")?.into()),
            "--skin" => args.skins.push(it.next().context("--skin 需要皮肤名")?),
            "--debug" => args.debug = true,
            "-h" | "--help" => {
                println!("用法: shimeji-next [--assets <资源目录>] [--skin <皮肤名>]... [--debug]");
                std::process::exit(0);
            }
            other => anyhow::bail!("未知参数: {other}"),
        }
    }
    Ok(args)
}

/// 依次尝试：命令行 → 环境变量 → .app 的 Resources → 可执行文件旁 → 当前目录 → 项目开发目录
fn find_assets(explicit: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    candidates.extend(explicit);
    candidates.extend(std::env::var_os("SHIMEJI_ASSETS").map(PathBuf::from));
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)) {
        candidates.push(dir.join("../Resources"));
        candidates.push(dir.join("assets"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("assets"));
    }
    candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets")));
    candidates
        .into_iter()
        .find(|p| Resources::is_valid(p))
        .map(|p| p.canonicalize().unwrap_or(p))
        .context("找不到资源目录（需要 i18n/en.toml 与 skins/），可用 --assets 指定")
}

fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    let root = find_assets(args.assets)?;
    let res = Resources::new(&root).with_user_skins(user_data_dir().join("skins"));
    eprintln!("[shimeji] 内置资源 {}", root.display());
    if let Some(dir) = &res.user_skins {
        eprintln!("[shimeji] 用户皮肤 {}", dir.display());
    }

    let mut settings = Settings::load();
    if !args.skins.is_empty() {
        // 只影响本次运行：之后若在菜单里改了设置才会写回
        settings.active_skins = args.skins;
    }
    let world = World::new(res, settings, shimeji_next::platform::create(), args.debug);
    shimeji_next::app::run(world)
}
