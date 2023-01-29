use std::cell::{RefCell};
use std::mem::{size_of, zeroed};
use std::ops::DerefMut;
use tao::event_loop::{EventLoop, EventLoopProxy};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, POINT, RECT, TRUE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MonitorFromPoint, MONITORINFO};
use windows_sys::Win32::UI::WindowsAndMessaging::{CallNextHookEx, GetCursorPos, HHOOK, MSLLHOOKSTRUCT, SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL, WM_MOUSEMOVE};
use windows::Win32::Graphics::Gdi::HMONITOR as WinHMonitor;
use crate::CustomEvent;

struct CursorTrackerContext {
    current_monitor: HMONITOR,
    current_monitor_info: MONITORINFO,
    event_loop_proxy: EventLoopProxy<CustomEvent>
}

thread_local! {static CONTEXT: RefCell<Option<CursorTrackerContext>> = RefCell::new(None)}

fn contains(rect: RECT, pt: POINT) -> bool {
    pt.x >= rect.left && pt.x <= rect.right &&
        pt.y >= rect.top  && pt.y <= rect.bottom
}

fn get_monitor_info(monitor: HMONITOR) -> Option<MONITORINFO> {
    unsafe {
        let mut info = MONITORINFO{
            cbSize: size_of::<MONITORINFO>() as u32,
            ..zeroed()
        };
        match GetMonitorInfoW(monitor, &mut info) {
            TRUE => Some(info),
            _ => None
        }
    }

}

unsafe extern "system" fn ll_mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if wparam as u32 == WM_MOUSEMOVE {
        let event = (lparam as *const MSLLHOOKSTRUCT).read();
        CONTEXT.with(|ctx| {
           if let Some(ctx) = ctx.borrow_mut().deref_mut() {
                if !contains(ctx.current_monitor_info.rcMonitor, event.pt) {
                    let monitor = MonitorFromPoint(event.pt, MONITOR_DEFAULTTONEAREST);
                    if monitor != ctx.current_monitor {
                        if let Some(info) = get_monitor_info(monitor) {
                            ctx.current_monitor_info = info;
                            ctx.current_monitor = monitor;
                            let monitor = WinHMonitor(monitor);
                            if let Err(e) = ctx.event_loop_proxy.send_event(CustomEvent::CursorMonitorSwitch(monitor)){
                                log::warn!("Cannot send event: {}", e);
                            }
                        }
                    }
                }
           }
        });
    }
    CallNextHookEx(0, code, wparam, lparam)
}

pub struct CursorTrackerHandle(HHOOK);

impl Drop for CursorTrackerHandle {
    fn drop(&mut self) {
        CONTEXT.with(|ctx| ctx.replace(None));
        let result = unsafe { UnhookWindowsHookEx(self.0) == TRUE };
        log::trace!("Removing mouse hook (successful: {})", result);
    }
}

fn get_current_monitor_sys() -> Option<HMONITOR> {
    unsafe {
        let mut pt = zeroed();
        match GetCursorPos(&mut pt) {
            TRUE => Some(MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST)),
            _ => None
        }
    }

}

pub fn get_current_monitor() -> WinHMonitor {
    WinHMonitor(get_current_monitor_sys().unwrap())
}

#[must_use]
pub fn set_hook(event_loop: &EventLoop<CustomEvent>) -> CursorTrackerHandle {
    let monitor = get_current_monitor_sys().unwrap();
    let info = get_monitor_info(monitor).unwrap();
    let result = CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        if ctx.is_none() {
            ctx.replace(CursorTrackerContext {
                current_monitor: monitor,
                current_monitor_info: info,
                event_loop_proxy: event_loop.create_proxy(),
            });
            true
        } else {
            false
        }
    });
    assert!(result);
    let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(ll_mouse_proc), 0, 0) };
    //CONTEXT.with(|ctx| ctx.borrow_mut().insert());
    CursorTrackerHandle(hook)
}