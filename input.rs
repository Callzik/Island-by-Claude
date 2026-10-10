//! Cursor, keyboard, foreground-window and window-style helpers.

use std::mem::size_of;

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_LBUTTON, VK_LWIN, VK_MENU, VK_R, VK_RBUTTON, VK_RWIN, VK_SHIFT, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindow, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    GWL_EXSTYLE, GWL_STYLE, SM_SWAPBUTTON, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SW_RESTORE, WS_CAPTION, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

use super::util::hwnd;

pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

fn key_down(vk: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000) != 0
}

/// The primary (logical left) button. GetAsyncKeyState reads physical buttons,
/// so with "swap mouse buttons" turned on the primary one is the right key.
pub fn lmb_down() -> bool {
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    key_down(if swapped { VK_RBUTTON } else { VK_LBUTTON })
}

/// True for any window of this process (island, capture overlay, tray menu…).
pub fn is_own_process(h: isize) -> bool {
    if h == 0 {
        return false;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd(h), Some(&mut pid)) };
    pid != 0 && pid == std::process::id()
}

pub fn foreground() -> isize {
    unsafe { GetForegroundWindow() }.0 as isize
}

pub fn is_window(h: isize) -> bool {
    h != 0 && unsafe { IsWindow(Some(hwnd(h))) }.as_bool()
}

pub fn class_name(h: isize) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd(h), &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Brings a previously active window back to the front.
pub fn activate(h: isize) {
    if !is_window(h) {
        return;
    }
    unsafe {
        if IsIconic(hwnd(h)).as_bool() {
            let _ = ShowWindow(hwnd(h), SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd(h));
    }
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Releases modifiers that may still be held and sends Ctrl+V.
pub fn send_paste() {
    let mut inputs: Vec<INPUT> = Vec::new();
    for vk in [VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN] {
        if key_down(vk) {
            inputs.push(key(vk, true));
        }
    }
    inputs.push(key(VK_CONTROL, false));
    inputs.push(key(VK_V, false));
    inputs.push(key(VK_V, true));
    inputs.push(key(VK_CONTROL, true));
    unsafe {
        SendInput(&inputs, size_of::<INPUT>() as i32);
    }
}

/// Presses `keys` in order and releases them in reverse (e.g. Win+Alt+R).
pub fn send_combo(keys: &[VIRTUAL_KEY]) {
    let mut inputs: Vec<INPUT> = Vec::new();
    for vk in [VK_MENU, VK_SHIFT, VK_CONTROL, VK_LWIN, VK_RWIN] {
        if key_down(vk) {
            inputs.push(key(vk, true));
        }
    }
    inputs.extend(keys.iter().map(|&k| key(k, false)));
    inputs.extend(keys.iter().rev().map(|&k| key(k, true)));
    unsafe {
        SendInput(&inputs, size_of::<INPUT>() as i32);
    }
}

/// Starts / stops Xbox Game Bar screen recording (Win+Alt+R).
pub fn toggle_game_bar_recording() {
    send_combo(&[VK_LWIN, VK_MENU, VK_R]);
}

/// Hides the window from Alt+Tab and the taskbar.
pub fn make_tool_window(h: isize) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd(h), GWL_EXSTYLE);
        let ex = (ex | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd(h), GWL_EXSTYLE, ex);
        let _ = SetWindowPos(
            hwnd(h),
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

fn monitor_rect_of(h: isize) -> Option<RECT> {
    unsafe {
        let mon = MonitorFromWindow(hwnd(h), MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(mon, &mut mi).as_bool() {
            Some(mi.rcMonitor)
        } else {
            None
        }
    }
}

/// True when the foreground window covers the whole monitor the island lives on
/// (games, videos, presentations).
pub fn foreground_is_fullscreen(own: isize) -> bool {
    let fg = foreground();
    // our own capture overlay covers the monitor too — that is not a game
    if fg == 0 || fg == own || is_own_process(fg) {
        return false;
    }
    let class = class_name(fg);
    if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
        return false;
    }
    // A maximized window with a title bar can cover the screen when the taskbar
    // auto-hides; real fullscreen (games, F11, video) drops the caption.
    let style = unsafe { GetWindowLongPtrW(hwnd(fg), GWL_STYLE) } as u32;
    if style & WS_CAPTION.0 == WS_CAPTION.0 {
        return false;
    }
    let (Some(mon), Some(own_mon)) = (monitor_rect_of(fg), monitor_rect_of(own)) else {
        return false;
    };
    if mon != own_mon {
        return false;
    }
    let mut r = RECT::default();
    if unsafe { GetWindowRect(hwnd(fg), &mut r) }.is_err() {
        return false;
    }
    r.left <= mon.left && r.top <= mon.top && r.right >= mon.right && r.bottom >= mon.bottom
}

pub fn lock_workstation() -> bool {
    unsafe { LockWorkStation() }.is_ok()
}
