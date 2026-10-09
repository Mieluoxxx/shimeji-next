//! winit 事件循环：60Hz 固定步长推进模拟，每只宠物一个透明置顶窗口。
//!
//! 点击穿透：每帧用全局光标 + 当前帧 alpha 判断是否压在不透明像素上，
//! 只有压在身上时窗口才接收鼠标，透明区域的点击落到下面的应用。

mod menu;
mod render;

use crate::sim::World;
use menu::{Command, MascotOp, Tray};
use render::{Gpu, Surface};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, MouseButton, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{CursorIcon, Window, WindowId, WindowLevel};

const STEP: f64 = 1.0 / 60.0;
/// 卡顿（如模态菜单）后最多补几步，超过就丢弃，避免宠物瞬移
const MAX_CATCH_UP: u32 = 4;

pub enum UserEvent {
    Menu(MenuEvent),
}

struct MascotWindow {
    window: Arc<Window>,
    surface: Surface,
    drawn: Option<(String, bool, u32, u32)>,
    pos: Option<(i32, i32)>,
    size: Option<(i32, i32)>,
    visible: bool,
    hittest: bool,
    hand: bool,
}

pub struct App {
    world: World,
    gpu: Option<Gpu>,
    windows: HashMap<u32, MascotWindow>,
    by_window: HashMap<WindowId, u32>,
    tray: Option<Tray>,
    /// 弹出的右键菜单必须在显示期间存活
    context_menu: Option<Menu>,
    next_frame: Instant,
    started: bool,
    pressed: Option<u32>,
    proxy: EventLoopProxy<UserEvent>,
}

pub fn run(world: World) -> anyhow::Result<()> {
    #[allow(unused_mut)]
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        // 桌宠不占 Dock 图标，只留菜单栏托盘
        builder.with_activation_policy(ActivationPolicy::Accessory);
    }
    let event_loop = builder.build()?;
    let mut app = App::new(world, event_loop.create_proxy());
    event_loop.run_app(&mut app)?;
    Ok(())
}

impl App {
    fn new(world: World, proxy: EventLoopProxy<UserEvent>) -> Self {
        App {
            world,
            gpu: None,
            windows: HashMap::new(),
            by_window: HashMap::new(),
            tray: None,
            context_menu: None,
            next_frame: Instant::now(),
            started: false,
            pressed: None,
            proxy,
        }
    }

    fn start(&mut self, event_loop: &ActiveEventLoop) {
        let proxy = self.proxy.clone();
        MenuEvent::set_event_handler(Some(move |e| {
            let _ = proxy.send_event(UserEvent::Menu(e));
        }));

        self.feed_monitors(event_loop);
        self.world.prime_env();
        for skin in self.world.active_skins() {
            self.world.spawn(&skin);
        }
        if !self.world.has_loaded_skins() {
            eprintln!("没有可用的皮肤，退出");
            event_loop.exit();
            return;
        }
        match Tray::new(&self.world) {
            Ok(t) => self.tray = Some(t),
            Err(e) => eprintln!("[tray] 创建托盘失败: {e:#}"),
        }
        self.next_frame = Instant::now();
    }

    /// 非 macOS 兜底：屏幕信息只能从 winit 拿
    fn feed_monitors(&mut self, event_loop: &ActiveEventLoop) {
        if cfg!(target_os = "macos") {
            return;
        }
        let primary = event_loop.primary_monitor();
        let screens = event_loop
            .available_monitors()
            .enumerate()
            .map(|(i, mon)| {
                let s = mon.scale_factor();
                let p = mon.position().to_logical::<f64>(s);
                let z = mon.size().to_logical::<f64>(s);
                let r = crate::sim::env::Rect {
                    x: p.x as i32,
                    y: p.y as i32,
                    w: z.width as i32,
                    h: z.height as i32,
                };
                crate::platform::ScreenInfo {
                    id: i as u32,
                    frame: r,
                    visible: r,
                    primary: primary.as_ref() == Some(&mon),
                }
            })
            .collect();
        self.world.platform.feed_screens(screens);
    }

    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let step = Duration::from_secs_f64(STEP);
        let now = Instant::now();
        let mut steps = 0;
        while self.next_frame <= now && steps < MAX_CATCH_UP {
            self.world.tick(STEP);
            self.next_frame += step;
            steps += 1;
        }
        if self.next_frame <= now {
            self.next_frame = now + step;
        }
        self.sync_windows(event_loop);
        self.update_hittest();
        if self.world.is_empty() {
            event_loop.exit();
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop, id: u32, x: i32, y: i32, w: i32, h: i32) -> bool {
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes()
            .with_title("shimeji-next")
            .with_transparent(true)
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_visible(false)
            .with_inner_size(LogicalSize::new(w.max(1), h.max(1)))
            .with_position(LogicalPosition::new(x, y));
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs = attrs.with_has_shadow(false);
        }
        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attrs = attrs.with_skip_taskbar(true);
        }
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("[window] 创建窗口失败: {e}");
                return false;
            }
        };
        self.world.platform.configure_mascot_window(&window);
        let surface = match &self.gpu {
            Some(gpu) => gpu.surface(window.clone()),
            None => Gpu::new(window.clone()).map(|(gpu, s)| {
                self.gpu = Some(gpu);
                s
            }),
        };
        let surface = match surface {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[render] {e:#}");
                event_loop.exit();
                return false;
            }
        };
        let _ = window.set_cursor_hittest(false);
        self.by_window.insert(window.id(), id);
        self.windows.insert(
            id,
            MascotWindow {
                window,
                surface,
                drawn: None,
                pos: Some((x, y)),
                size: Some((w, h)),
                visible: false,
                hittest: false,
                hand: false,
            },
        );
        true
    }

    /// 把模拟结果落到窗口：位置、尺寸、贴图
    fn sync_windows(&mut self, event_loop: &ActiveEventLoop) {
        let live: HashSet<u32> = self.world.live().map(|m| m.id).collect();
        let by_window = &mut self.by_window;
        self.windows.retain(|id, w| {
            let keep = live.contains(id);
            if !keep {
                by_window.remove(&w.window.id());
            }
            keep
        });
        if self.pressed.is_some_and(|id| !live.contains(&id)) {
            self.pressed = None;
        }

        let frames: Vec<_> = self
            .world
            .live()
            .filter(|m| !m.paused)
            .map(|m| (m.id, m.image.clone(), m.image_bounds()))
            .collect();

        for (id, image, bounds) in frames {
            let (Some(img), Some(b)) = (image, bounds) else {
                if let Some(w) = self.windows.get_mut(&id) {
                    if w.visible {
                        w.window.set_visible(false);
                        w.visible = false;
                    }
                }
                continue;
            };
            if !self.windows.contains_key(&id) && !self.create_window(event_loop, id, b.x, b.y, b.w, b.h) {
                continue;
            }
            let Some(w) = self.windows.get_mut(&id) else { continue };
            if w.size != Some((b.w, b.h)) {
                let _ = w.window.request_inner_size(LogicalSize::new(b.w.max(1), b.h.max(1)));
                w.size = Some((b.w, b.h));
            }
            if w.pos != Some((b.x, b.y)) {
                w.window.set_outer_position(LogicalPosition::new(b.x, b.y));
                w.pos = Some((b.x, b.y));
            }
            let phys = w.window.inner_size();
            let key = (img.sprite.key.clone(), img.flipped, phys.width, phys.height);
            if w.drawn.as_ref() != Some(&key) {
                if let Some(gpu) = &mut self.gpu {
                    gpu.draw(&mut w.surface, phys.width, phys.height, &img.sprite, img.flipped);
                }
                w.drawn = Some(key);
            }
            if !w.visible {
                w.window.set_visible(true);
                w.visible = true;
            }
        }
    }

    fn update_hittest(&mut self) {
        let (cx, cy) = self.world.cursor();
        let (cx, cy) = (cx.floor() as i32, cy.floor() as i32);
        for (id, w) in &mut self.windows {
            let Some(m) = self.world.mascot(*id) else { continue };
            let Some(b) = m.image_bounds() else { continue };
            let (lx, ly) = (cx - b.x, cy - b.y);
            let inside = lx >= 0 && ly >= 0 && lx < b.w && ly < b.h;
            let opaque = inside && m.image.as_ref().is_some_and(|img| img.alpha_at(lx, ly) > 0);
            let want = opaque || self.pressed == Some(*id) || m.is_dragged();
            if want != w.hittest {
                let _ = w.window.set_cursor_hittest(want);
                w.hittest = want;
            }
            let hand = opaque && !m.is_dragged();
            if hand != w.hand {
                w.window.set_cursor(if hand { CursorIcon::Grab } else { CursorIcon::Default });
                w.hand = hand;
            }
        }
    }

    fn show_context_menu(&mut self, id: u32) {
        let Some(menu) = menu::mascot_menu(&self.world, id) else { return };
        let Some(w) = self.windows.get(&id) else { return };
        #[cfg(target_os = "macos")]
        {
            use tray_icon::menu::ContextMenu;
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = w.window.window_handle() {
                if let RawWindowHandle::AppKit(h) = h.as_raw() {
                    unsafe {
                        menu.show_context_menu_for_nsview(h.ns_view.as_ptr(), None);
                    }
                }
            }
        }
        #[cfg(target_os = "windows")]
        {
            use tray_icon::menu::ContextMenu;
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = w.window.window_handle() {
                if let RawWindowHandle::Win32(h) = h.as_raw() {
                    unsafe {
                        menu.show_context_menu_for_hwnd(h.hwnd.get(), None);
                    }
                }
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let _ = w;
        self.context_menu = Some(menu);
    }

    fn handle_command(&mut self, cmd: Command, event_loop: &ActiveEventLoop) {
        let w = &mut self.world;
        match cmd {
            Command::Call => w.spawn_random(),
            Command::Follow => w.set_behavior_all("ChaseMouse", None),
            Command::ReduceToOne => w.remain_one(),
            Command::Restore => w.restore_ie(),
            Command::PauseAll => w.toggle_pause_all(),
            Command::OpenSkins => {
                if let Some(dir) = &w.res.user_skins {
                    open_in_file_manager(dir);
                }
            }
            Command::Rescan => {
                // 托盘菜单项是建菜单时固定下来的，新放进来的皮肤要重建菜单才出现
                self.tray = None;
                match Tray::new(&self.world) {
                    Ok(t) => self.tray = Some(t),
                    Err(e) => eprintln!("[tray] 重建托盘失败: {e:#}"),
                }
                return;
            }
            Command::Quit => {
                w.dispose_all();
                event_loop.exit();
            }
            Command::Toggle(flag) => {
                let s = &mut w.settings;
                let slot = match flag {
                    menu::Flag::Breeding => &mut s.breeding,
                    menu::Flag::Throwing => &mut s.throwing,
                    menu::Flag::Multiscreen => &mut s.multiscreen,
                };
                *slot = !*slot;
                let on = *slot;
                s.save();
                if matches!(flag, menu::Flag::Throwing) && on && !w.platform.can_move_windows() {
                    w.platform.request_window_permission();
                }
            }
            Command::Skin(skin) => {
                let mut active = w.active_skins();
                if let Some(i) = active.iter().position(|s| *s == skin) {
                    if active.len() > 1 {
                        active.remove(i);
                        w.dispose_skin(&skin);
                    }
                } else {
                    active.push(skin.clone());
                    w.spawn(&skin);
                }
                w.settings.active_skins = active;
                w.settings.save();
            }
            Command::Mascot(id, op) => match op {
                MascotOp::CallAnother => {
                    if let Some(skin) = w.mascot(id).map(|m| m.skin.clone()) {
                        w.spawn(&skin);
                    }
                }
                MascotOp::Follow => {
                    let skin = w.mascot(id).map(|m| m.skin.clone());
                    w.set_behavior_all("ChaseMouse", skin.as_deref());
                }
                MascotOp::Restore => w.restore_ie(),
                MascotOp::SetBehavior(name) => w.set_behavior_of(id, &name),
                MascotOp::ToggleBehavior(name) => {
                    if let Some(skin) = w.mascot(id).map(|m| m.skin.clone()) {
                        w.settings.toggle_behavior(&skin, &name);
                    }
                }
                MascotOp::Pause => w.toggle_pause(id),
                MascotOp::Dismiss => w.dispose(id),
                MascotOp::DismissOthers => w.remain_one_of_skin(id),
                MascotOp::DismissAllOthers => w.remain_only(id),
            },
        }
        if let Some(t) = &self.tray {
            t.refresh(&self.world);
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.started {
            self.started = true;
            self.start(event_loop);
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if self.started && !matches!(cause, StartCause::Init) && Instant::now() >= self.next_frame {
            if !cfg!(target_os = "macos") {
                self.feed_monitors(event_loop);
            }
            self.frame(event_loop);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(e) => {
                if let Some(cmd) = Command::parse(&e.id.0) {
                    self.handle_command(cmd, event_loop);
                }
            }
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let Some(&id) = self.by_window.get(&window_id) else { return };
        match event {
            WindowEvent::CloseRequested => self.world.dispose(id),
            WindowEvent::CursorMoved { position, .. } => {
                // 非 macOS 兜底平台拿不到全局光标，用窗口内坐标换算
                let Some(w) = self.windows.get(&id) else { return };
                let local = position.to_logical::<f64>(w.window.scale_factor());
                let (px, py) = w.pos.unwrap_or((0, 0));
                self.world.platform.feed_cursor(px + local.x as i32, py + local.y as i32);
            }
            WindowEvent::MouseInput { state, button, .. } => match (button, state) {
                (MouseButton::Left, ElementState::Pressed) => {
                    self.pressed = Some(id);
                    self.world.press(id);
                }
                (MouseButton::Left, ElementState::Released) => {
                    self.pressed = None;
                    self.world.release(id);
                }
                (MouseButton::Right, ElementState::Released) => {
                    self.show_context_menu(id);
                    // 模态菜单期间模拟停摆，回来后从当前时刻继续
                    self.next_frame = Instant::now();
                }
                _ => {}
            },
            WindowEvent::ScaleFactorChanged { .. } | WindowEvent::Resized(_) | WindowEvent::RedrawRequested => {
                if let Some(w) = self.windows.get_mut(&id) {
                    w.drawn = None;
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }
}

fn open_in_file_manager(dir: &std::path::Path) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(opener).arg(dir).spawn() {
        eprintln!("[skins] 打开 {} 失败: {e}", dir.display());
    }
}
