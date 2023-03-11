use std::marker::PhantomData;
use windows::core::HSTRING;
use anyhow::Result;
use error_tools::SomeOptionExt;
use windows::Win32::Foundation::{FALSE, TRUE};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};

fn find_terminal_idx(content: &[u16]) -> usize {
    for (i, val) in content.iter().enumerate() {
        if *val == 0 {
            return i;
        }
    }
    content.len()
}

pub fn convert_u16_to_string(data: &[u16]) -> String {
    let terminal_idx = find_terminal_idx(data);
    HSTRING::from_wide(&data[0..terminal_idx]).unwrap().to_string_lossy()
}

pub fn make_blend_state(device: &ID3D11Device, src: D3D11_BLEND, dst: D3D11_BLEND) -> Result<ID3D11BlendState> {
    make_resource(|ptr| unsafe {
        device.CreateBlendState(&D3D11_BLEND_DESC {
            RenderTarget: [D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: TRUE,
                SrcBlend: src,
                DestBlend: dst,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_INV_DEST_ALPHA,
                DestBlendAlpha: D3D11_BLEND_ONE,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as _,
            }; 8],
            IndependentBlendEnable: FALSE,
            AlphaToCoverageEnable: FALSE
        }, ptr)
    })
}

pub fn make_resource<T>(func: impl FnOnce(Option<*mut Option<T>>) -> windows::core::Result<()>) -> anyhow::Result<T> {
    let mut obj = None;
    func(Some(&mut obj))?;
    Ok(obj.some()?)
}

pub fn retrieve<S, T>(self_type: &S, func: unsafe fn(&S, *mut T)) -> T {
    unsafe {
        let mut desc = std::mem::MaybeUninit::zeroed();
        func(self_type, desc.as_mut_ptr());
        desc.assume_init()
    }
}

#[derive(Default)]
struct ComWrapper {
    _ptr: PhantomData<*mut ()>,
}

thread_local!(static COM_INITIALIZED: ComWrapper = {
    unsafe {
        SetProcessDpiAwarenessContext(Some(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2));
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .expect("Could not initialize COM");
        let thread = std::thread::current();
        log::trace!("Initialized COM on thread \"{}\"", thread.name().unwrap_or(""));
        ComWrapper::default()
    }
});

impl Drop for ComWrapper {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
            let thread = std::thread::current();
            log::trace!("Uninitialized COM on thread \"{}\"", thread.name().unwrap_or(""));
        }
    }
}

#[inline]
pub fn com_initialized() {
    COM_INITIALIZED.with(|_| {});
}

pub fn show_message_box<T1: Into<HSTRING>, T2: Into<HSTRING>>(title: T1, msg: T2) where {
    unsafe {
        MessageBoxW(None, &msg.into(), &title.into(), MB_OK | MB_ICONERROR);
    }
}

pub struct U8Iter {
    value: u8,
    size: u32
}

impl U8Iter {
    pub fn new(value: u8) -> Self {
        Self {
            value,
            size: u8::BITS,
        }
    }
}

impl Iterator for U8Iter {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if self.size > 0 {
            let result = self.value & 0x80 != 0x0;
            self.size -= 1;
            self.value <<= 1;
            Some(result)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (u8::BITS as usize, Some(u8::BITS as usize))
    }
}

impl ExactSizeIterator for U8Iter {}

