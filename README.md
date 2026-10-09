# Shimeji Next

English | [简体中文](README_zh.md)

A native Rust desktop mascot engine. Frame-rate-independent physics at 60fps, compatible with the Shimeji community's 46-frame skins: drop a folder containing `shime1.png`–`shime46.png` into the skins directory and it just works.

## Download

Get builds from [Releases](https://github.com/Mieluoxxx/shimeji-next/releases):

- **macOS (Apple Silicon)**: `ShimejiNext-<version>-arm64.dmg`. Not notarized, so right-click → Open on first launch.
- **Windows (experimental)**: `ShimejiNext-<version>-windows-x64-portable.zip`. Unzip and run `shimeji-next.exe`. There is no native Windows platform layer yet (taskbar and window interaction are not supported), and transparency depends on the GPU driver.

## Development

```bash
cargo run --release -- --skin Shimeji --debug   # assets are loaded from assets/
cargo test --release                           # unit tests + windowless behavior simulation in tests/
```

## Packaging

```bash
./build_app.sh --universal   # target/dist/Shimeji Next.app + ShimejiNext-<version>-universal.dmg
```

For release signing/notarization, set `SIGN_IDENTITY` and `NOTARY_PROFILE`; see the header comment of the script.

## Project layout

```
src/
├── main.rs            CLI entry: locate assets, load settings, launch
├── lib.rs
├── sim.rs  sim/       Platform-independent simulation (testable without windows)
│   ├── env.rs         Screens / work area / foreground window / cursor, border semantics
│   ├── clips.rs       Animation clips for the 46 frames (seconds, px/s)
│   ├── engine.rs      Behavior engine: step state machine, physics, dragging
│   ├── behaviors.rs   Behavior table: conditions, weights, follow-up chains
│   ├── mascot.rs
│   └── world.rs       Scheduling, breeding, interaction
├── assets.rs  assets/ Asset lookup, skins and skin.toml, frame cache
├── platform.rs  platform/   OS integration (macOS: NSScreen, CGEvent, CGWindowList, AX)
├── app.rs  app/       Event loop, windows, wgpu rendering, tray and context menus
└── settings.rs        settings.toml and i18n
tests/simulation.rs    Integration tests
assets/
├── i18n/*.toml        UI strings
├── skins/<name>/      Built-in skins
├── icon.png           Tray icon
└── bundled-skins.txt  Skins packaged into the .app
```

## Skins

> **Supported format**: only the common PC Shimeji-ee 46-frame format (`shime1.png`–`shime46.png`) is supported.
> Skin packs exported from the mobile Shimeji app (the 70-frame format with `manifest.json`, `animation.json` and `sprites/0000.webp`–`0069.webp`) are **not supported**.
> When taking a skin from a Shimeji-ee distribution, copy only the `img/<skin name>/` folder; the bundled `conf/actions.xml` and `behaviors.xml` are not read.

- Built-in: `assets/skins/`; `assets/bundled-skins.txt` decides which ones get packaged
- User: `~/Library/Application Support/shimeji-next/skins/` (tray → Choose Skins → Open Skins Folder); a user skin overrides a built-in one with the same name
- Optional `skin.toml`:

```toml
name = "Angela"        # Name shown in the tray's skin list; defaults to the folder name
author = "..."
license = "Personal use only"
scale = 1.5            # Multiplied with the global scaling
speed = 1.2            # Speed multiplier for walking / running / climbing

[anchors]              # Frame number = [foot x, y], overrides the default anchor
19 = [64, 104]

[behaviors]
disabled = ["SplitIntoTwo", "PullUpShimeji"]
weights = { SitDown = 400 }
```

## Settings

`~/Library/Application Support/shimeji-next/settings.toml`; changes made from the tray menu are saved automatically.

## License and credits

AGPL-3.0-only, see [LICENSE](LICENSE).

Behavior design and the frame convention come from Shimeji-ee (original author Yuki Yamada / Group Finity, maintained by the Shimeji-ee Group), released upstream under GPL-3.0; per GPL-3.0 section 13, this project as a whole is distributed under AGPL-3.0. Skin artwork is copyright of its respective authors and is not covered by this license.
