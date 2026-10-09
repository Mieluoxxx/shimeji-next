# Shimeji Next

[English](README.md) | 简体中文

原生 Rust 桌宠引擎。60fps 帧率无关物理，兼容 Shimeji 社区的 46 帧皮肤：把含 `shime1.png`~`shime46.png` 的文件夹放进皮肤目录就能用。

## 开发

```bash
cargo run --release -- --skin Shimeji --debug   # 资源取自 assets/
cargo test --release                           # 单元测试 + tests/ 下的无窗口行为仿真
```

## 打包

```bash
./build_app.sh --universal   # target/dist/Shimeji Next.app + ShimejiNext-<版本>-universal.dmg
```

正式签名/公证：设置 `SIGN_IDENTITY` 与 `NOTARY_PROFILE`，详见脚本头注释。

## 目录结构

```
src/
├── main.rs            命令行入口：定位资源、加载设置、启动
├── lib.rs
├── sim.rs  sim/       平台无关的模拟层（可无窗口测试）
│   ├── env.rs         屏幕/工作区/前台窗口/光标与边界语义
│   ├── clips.rs       46 帧的动画剪辑（秒、像素/秒）
│   ├── engine.rs      行为执行引擎：Step 状态机、物理、拖拽
│   ├── behaviors.rs   行为表：条件、权重、后续行为链
│   ├── mascot.rs
│   └── world.rs       调度、繁殖、交互
├── assets.rs  assets/ 资源定位、皮肤与 skin.toml、帧图缓存
├── platform.rs  platform/   系统接口（macOS：NSScreen、CGEvent、CGWindowList、AX）
├── app.rs  app/       事件循环、窗口、wgpu 渲染、托盘与右键菜单
└── settings.rs        settings.toml 与 i18n
tests/simulation.rs    集成测试
assets/
├── i18n/*.toml        界面文案
├── skins/<名字>/      内置皮肤
├── icon.png           托盘图标
└── bundled-skins.txt  打包进 .app 的皮肤
```

## 皮肤

- 内置：`assets/skins/`，由 `assets/bundled-skins.txt` 决定打包哪些
- 用户：`~/Library/Application Support/shimeji-next/skins/`（托盘「选择皮肤 → 打开皮肤文件夹」），同名覆盖内置
- 可选 `skin.toml`：

```toml
name = "安吉拉"         # 托盘「选择皮肤」里显示的名字，缺省用文件夹名
author = "..."
license = "仅限个人使用"
scale = 1.5            # 叠加在全局缩放上
speed = 1.2            # 走/跑/爬的速度倍率

[anchors]              # 帧号 = [脚底 x, y]，覆盖默认锚点
19 = [64, 104]

[behaviors]
disabled = ["SplitIntoTwo", "PullUpShimeji"]
weights = { SitDown = 400 }
```

## 设置

`~/Library/Application Support/shimeji-next/settings.toml`，托盘菜单里改动会自动写回。

## 许可与致谢

AGPL-3.0-only，见 [LICENSE](LICENSE)。

行为设计与帧约定源自 Shimeji-ee（原作者 Yuki Yamada / Group Finity，Shimeji-ee Group 维护），上游以 GPL-3.0 发布；依 GPL-3.0 第 13 条，本项目整体以 AGPL-3.0 分发。皮肤图像版权归各自作者，不受本许可证约束。
