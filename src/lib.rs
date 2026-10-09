//! Shimeji Next：原生 Rust 桌宠引擎。
//!
//! - `sim`：与平台无关的模拟（环境、行为、物理），可无窗口测试
//! - `assets`：资源定位、皮肤与帧图加载
//! - `platform`：屏幕/光标/前台窗口等系统接口
//! - `app`：窗口、渲染、托盘与菜单

pub mod app;
pub mod assets;
pub mod platform;
pub mod settings;
pub mod sim;
