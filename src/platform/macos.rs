//! macOS：NSScreen 取工作区，CGEvent 取全局光标，CGWindowList 取前台窗口
//! （只读边界不需要任何授权），移动窗口走 Accessibility API（需要用户授权）。

use super::{Platform, ScreenInfo, WindowInfo};
use crate::sim::env::Rect;
use objc2_app_kit::{NSScreen, NSView, NSWindowCollectionBehavior};
use objc2_foundation::MainThreadMarker;
use std::collections::HashSet;
use std::ffi::{c_char, c_void, CStr, CString};
use std::time::{Duration, Instant};

type CFTypeRef = *const c_void;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFArrayGetCount(a: CFTypeRef) -> isize;
    fn CFArrayGetValueAtIndex(a: CFTypeRef, i: isize) -> CFTypeRef;
    fn CFDictionaryGetValue(d: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
    fn CFDictionaryCreate(
        alloc: CFTypeRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        n: isize,
        key_cb: *const c_void,
        value_cb: *const c_void,
    ) -> CFTypeRef;
    fn CFNumberGetValue(n: CFTypeRef, ty: isize, out: *mut c_void) -> u8;
    fn CFStringCreateWithCString(alloc: CFTypeRef, s: *const c_char, enc: u32) -> CFTypeRef;
    fn CFStringGetCString(s: CFTypeRef, buf: *mut c_char, size: isize, enc: u32) -> u8;
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    static kCFBooleanTrue: CFTypeRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFTypeRef;
    fn CGRectMakeWithDictionaryRepresentation(dict: CFTypeRef, rect: *mut CGRect) -> u8;
    fn CGEventCreate(source: CFTypeRef) -> CFTypeRef;
    fn CGEventGetLocation(event: CFTypeRef) -> CGPoint;
    static kCGWindowLayer: CFTypeRef;
    static kCGWindowOwnerPID: CFTypeRef;
    static kCGWindowBounds: CFTypeRef;
    static kCGWindowAlpha: CFTypeRef;
    static kCGWindowName: CFTypeRef;
    static kCGWindowOwnerName: CFTypeRef;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> u8;
    static kAXTrustedCheckOptionPrompt: CFTypeRef;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(el: CFTypeRef, attr: CFTypeRef, out: *mut CFTypeRef) -> i32;
    fn AXUIElementSetAttributeValue(el: CFTypeRef, attr: CFTypeRef, value: CFTypeRef) -> i32;
    fn AXValueCreate(ty: u32, ptr: *const c_void) -> CFTypeRef;
    fn AXValueGetValue(v: CFTypeRef, ty: u32, out: *mut c_void) -> u8;
}

const UTF8: u32 = 0x0800_0100;
const NUMBER_SINT64: isize = 4;
const NUMBER_DOUBLE: isize = 13;
const LIST_ON_SCREEN_ONLY: u32 = 1 << 0;
const LIST_EXCLUDE_DESKTOP: u32 = 1 << 4;
const AX_POINT: u32 = 1;
const AX_SIZE: u32 = 2;

fn cfstr(s: &str) -> CFTypeRef {
    let c = CString::new(s).unwrap_or_default();
    unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), UTF8) }
}

unsafe fn dict_i64(d: CFTypeRef, key: CFTypeRef) -> Option<i64> {
    let v = CFDictionaryGetValue(d, key);
    if v.is_null() {
        return None;
    }
    let mut out = 0i64;
    (CFNumberGetValue(v, NUMBER_SINT64, &mut out as *mut i64 as *mut c_void) != 0).then_some(out)
}

unsafe fn dict_f64(d: CFTypeRef, key: CFTypeRef) -> Option<f64> {
    let v = CFDictionaryGetValue(d, key);
    if v.is_null() {
        return None;
    }
    let mut out = 0f64;
    (CFNumberGetValue(v, NUMBER_DOUBLE, &mut out as *mut f64 as *mut c_void) != 0).then_some(out)
}

unsafe fn dict_string(d: CFTypeRef, key: CFTypeRef) -> Option<String> {
    let v = CFDictionaryGetValue(d, key);
    if v.is_null() || CFGetTypeID(v) != CFStringGetTypeID() {
        return None;
    }
    let mut buf = vec![0 as c_char; 1024];
    if CFStringGetCString(v, buf.as_mut_ptr(), buf.len() as isize, UTF8) == 0 {
        return None;
    }
    Some(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
}

fn to_rect(r: CGRect) -> Rect {
    Rect {
        x: r.origin.x.round() as i32,
        y: r.origin.y.round() as i32,
        w: r.size.width.round() as i32,
        h: r.size.height.round() as i32,
    }
}

pub struct MacPlatform {
    pid: i64,
    ie_pid: Option<i32>,
    touched: HashSet<i32>,
    screens: Vec<ScreenInfo>,
    screens_at: Option<Instant>,
    trusted: bool,
    trusted_at: Option<Instant>,
    ax_focused: CFTypeRef,
    ax_position: CFTypeRef,
    ax_size: CFTypeRef,
    ax_windows: CFTypeRef,
}

impl MacPlatform {
    pub fn new() -> Self {
        MacPlatform {
            pid: std::process::id() as i64,
            ie_pid: None,
            touched: HashSet::new(),
            screens: Vec::new(),
            screens_at: None,
            trusted: false,
            trusted_at: None,
            ax_focused: cfstr("AXFocusedWindow"),
            ax_position: cfstr("AXPosition"),
            ax_size: cfstr("AXSize"),
            ax_windows: cfstr("AXWindows"),
        }
    }

    fn query_screens() -> Vec<ScreenInfo> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Vec::new();
        };
        let screens = NSScreen::screens(mtm);
        let Some(primary_h) = screens.iter().next().map(|s| s.frame().size.height) else {
            return Vec::new();
        };
        // Cocoa 是主屏左下原点 y 向上，翻成与 CGWindowList/CGEvent 一致的左上原点
        let flip = |r: objc2_foundation::NSRect| Rect {
            x: r.origin.x.round() as i32,
            y: (primary_h - (r.origin.y + r.size.height)).round() as i32,
            w: r.size.width.round() as i32,
            h: r.size.height.round() as i32,
        };
        screens
            .iter()
            .enumerate()
            .map(|(i, s)| ScreenInfo {
                id: i as u32,
                frame: flip(s.frame()),
                visible: flip(s.visibleFrame()),
                primary: i == 0,
            })
            .collect()
    }

    fn with_focused_window(&self, pid: i32, f: impl FnOnce(CFTypeRef)) {
        unsafe {
            let app = AXUIElementCreateApplication(pid);
            if app.is_null() {
                return;
            }
            let mut win: CFTypeRef = std::ptr::null();
            if AXUIElementCopyAttributeValue(app, self.ax_focused, &mut win) == 0 && !win.is_null() {
                f(win);
                CFRelease(win);
            }
            CFRelease(app);
        }
    }

    unsafe fn set_position(&self, win: CFTypeRef, x: f64, y: f64) {
        let p = CGPoint { x, y };
        let v = AXValueCreate(AX_POINT, &p as *const CGPoint as *const c_void);
        if !v.is_null() {
            AXUIElementSetAttributeValue(win, self.ax_position, v);
            CFRelease(v);
        }
    }

    unsafe fn ax_rect(&self, win: CFTypeRef) -> Option<Rect> {
        let mut pos = CGPoint::default();
        let mut size = CGSize::default();
        let mut v: CFTypeRef = std::ptr::null();
        if AXUIElementCopyAttributeValue(win, self.ax_position, &mut v) != 0 || v.is_null() {
            return None;
        }
        AXValueGetValue(v, AX_POINT, &mut pos as *mut CGPoint as *mut c_void);
        CFRelease(v);
        if AXUIElementCopyAttributeValue(win, self.ax_size, &mut v) != 0 || v.is_null() {
            return None;
        }
        AXValueGetValue(v, AX_SIZE, &mut size as *mut CGSize as *mut c_void);
        CFRelease(v);
        Some(to_rect(CGRect { origin: pos, size }))
    }
}

impl Platform for MacPlatform {
    fn screens(&mut self) -> Vec<ScreenInfo> {
        // Dock 自动隐藏/分辨率切换会改 visibleFrame，但没必要每 40ms 问一次
        let stale = self
            .screens_at
            .is_none_or(|t| t.elapsed() > Duration::from_millis(500));
        if stale {
            let fresh = Self::query_screens();
            if !fresh.is_empty() {
                self.screens = fresh;
            }
            self.screens_at = Some(Instant::now());
        }
        self.screens.clone()
    }

    fn cursor(&mut self) -> (i32, i32) {
        unsafe {
            let ev = CGEventCreate(std::ptr::null());
            if ev.is_null() {
                return (0, 0);
            }
            let p = CGEventGetLocation(ev);
            CFRelease(ev);
            (p.x.floor() as i32, p.y.floor() as i32)
        }
    }

    fn active_window(&mut self, screens: &[ScreenInfo]) -> Option<WindowInfo> {
        unsafe {
            let list = CGWindowListCopyWindowInfo(LIST_ON_SCREEN_ONLY | LIST_EXCLUDE_DESKTOP, 0);
            if list.is_null() {
                return None;
            }
            let mut found = None;
            for i in 0..CFArrayGetCount(list) {
                let d = CFArrayGetValueAtIndex(list, i);
                if dict_i64(d, kCGWindowLayer) != Some(0) {
                    continue;
                }
                let Some(pid) = dict_i64(d, kCGWindowOwnerPID) else {
                    continue;
                };
                if pid == self.pid || dict_f64(d, kCGWindowAlpha).unwrap_or(1.0) <= 0.0 {
                    continue;
                }
                let bounds = CFDictionaryGetValue(d, kCGWindowBounds);
                let mut r = CGRect::default();
                if bounds.is_null() || CGRectMakeWithDictionaryRepresentation(bounds, &mut r) == 0 {
                    continue;
                }
                let rect = to_rect(r);
                // 一些 App 在 layer 0 挂着不可见的小辅助窗口
                if rect.w < 64 || rect.h < 64 {
                    continue;
                }
                let title = dict_string(d, kCGWindowName)
                    .filter(|t| !t.is_empty())
                    .or_else(|| dict_string(d, kCGWindowOwnerName))
                    .unwrap_or_default();
                // 原版：与可视区相交、且不是铺满全屏的窗口才算可交互
                let visible = screens
                    .iter()
                    .any(|s| rect.intersects(&s.visible) && !rect.contains_rect(&s.visible));
                self.ie_pid = Some(pid as i32);
                found = Some(WindowInfo { rect, visible, title });
                break;
            }
            CFRelease(list);
            found
        }
    }

    fn can_move_windows(&mut self) -> bool {
        let stale = self
            .trusted_at
            .is_none_or(|t| t.elapsed() > Duration::from_secs(2));
        if stale {
            self.trusted = unsafe { AXIsProcessTrusted() != 0 };
            self.trusted_at = Some(Instant::now());
        }
        self.trusted
    }

    fn request_window_permission(&mut self) {
        unsafe {
            let keys = [kAXTrustedCheckOptionPrompt];
            let values = [kCFBooleanTrue];
            let opts = CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            self.trusted = AXIsProcessTrustedWithOptions(opts) != 0;
            if !opts.is_null() {
                CFRelease(opts);
            }
        }
        self.trusted_at = Some(Instant::now());
    }

    fn move_active_window(&mut self, x: i32, y: i32, current: Rect, bounds: Rect) {
        if !self.can_move_windows() {
            return;
        }
        let Some(pid) = self.ie_pid else { return };
        // 窗口整体拖出可视区会被系统弹回，原版也是先夹到能移动到的极限位置
        let px = (x as f64).clamp((bounds.x - current.w) as f64, bounds.right() as f64);
        let py = (y as f64).clamp(bounds.y as f64, bounds.bottom() as f64);
        self.with_focused_window(pid, |win| unsafe { self.set_position(win, px, py) });
        self.touched.insert(pid);
    }

    fn restore_windows(&mut self, bounds: Rect) {
        if !self.can_move_windows() {
            return;
        }
        for &pid in &self.touched {
            unsafe {
                let app = AXUIElementCreateApplication(pid);
                if app.is_null() {
                    continue;
                }
                let mut wins: CFTypeRef = std::ptr::null();
                if AXUIElementCopyAttributeValue(app, self.ax_windows, &mut wins) == 0 && !wins.is_null() {
                    for i in 0..CFArrayGetCount(wins) {
                        let w = CFArrayGetValueAtIndex(wins, i);
                        if let Some(r) = self.ax_rect(w) {
                            if !r.intersects(&bounds) {
                                self.set_position(w, (bounds.x + 20) as f64, (bounds.y + 20) as f64);
                            }
                        }
                    }
                    CFRelease(wins);
                }
                CFRelease(app);
            }
        }
        self.touched.clear();
    }

    fn configure_mascot_window(&self, window: &winit::window::Window) {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Ok(handle) = window.window_handle() else { return };
        let RawWindowHandle::AppKit(h) = handle.as_raw() else { return };
        // 宠物要跟着用户切换桌面空间，也不该出现在 Cmd+` 窗口循环里
        let view: &NSView = unsafe { &*(h.ns_view.as_ptr() as *const NSView) };
        if let Some(ns_window) = view.window() {
            ns_window.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::IgnoresCycle
                    | NSWindowCollectionBehavior::FullScreenAuxiliary,
            );
            ns_window.setHasShadow(false);
        }
    }
}
