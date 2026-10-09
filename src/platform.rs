//! 平台差异全部收敛在这里：屏幕/工作区、全局光标、前台窗口（原版的 "IE"）及其移动。

use crate::sim::env::Rect;

#[cfg(target_os = "macos")]
mod macos;

#[derive(Debug, Clone)]
pub struct ScreenInfo {
    pub id: u32,
    pub frame: Rect,
    /// 扣除菜单栏/Dock/任务栏后的可用区；宠物站在 Dock 上而不是被它挡住
    pub visible: Rect,
    pub primary: bool,
}

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub rect: Rect,
    pub visible: bool,
    pub title: String,
}

pub trait Platform {
    fn screens(&mut self) -> Vec<ScreenInfo>;
    fn cursor(&mut self) -> (i32, i32);
    fn active_window(&mut self, screens: &[ScreenInfo]) -> Option<WindowInfo>;
    /// 扔窗口一族（ThrowIE/WalkWithIE/FallWithIE）需要系统授权，没有就整体禁用
    fn can_move_windows(&mut self) -> bool;
    fn request_window_permission(&mut self) {}
    fn move_active_window(&mut self, x: i32, y: i32, current: Rect, bounds: Rect);
    fn restore_windows(&mut self, bounds: Rect);
    fn configure_mascot_window(&self, _window: &winit::window::Window) {}
    /// 非 macOS 的兜底实现拿不到全局光标，只能靠窗口事件喂
    fn feed_cursor(&mut self, _x: i32, _y: i32) {}
    fn feed_screens(&mut self, _screens: Vec<ScreenInfo>) {}
}

pub fn create() -> Box<dyn Platform> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacPlatform::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(Generic::default())
    }
}

/// 没有原生实现的平台：屏幕来自 winit，光标只在宠物窗口内可见，没有前台窗口交互
#[cfg_attr(target_os = "macos", allow(dead_code))]
#[derive(Default)]
pub struct Generic {
    screens: Vec<ScreenInfo>,
    cursor: (i32, i32),
}

impl Platform for Generic {
    fn screens(&mut self) -> Vec<ScreenInfo> {
        self.screens.clone()
    }
    fn cursor(&mut self) -> (i32, i32) {
        self.cursor
    }
    fn active_window(&mut self, _screens: &[ScreenInfo]) -> Option<WindowInfo> {
        None
    }
    fn can_move_windows(&mut self) -> bool {
        false
    }
    fn move_active_window(&mut self, _x: i32, _y: i32, _current: Rect, _bounds: Rect) {}
    fn restore_windows(&mut self, _bounds: Rect) {}
    fn feed_cursor(&mut self, x: i32, y: i32) {
        self.cursor = (x, y);
    }
    fn feed_screens(&mut self, screens: Vec<ScreenInfo>) {
        self.screens = screens;
    }
}
