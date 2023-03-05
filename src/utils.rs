use windows::core::{HSTRING, InParam};
use anyhow::Result;
use error_tools::SomeOptionExt;
use windows::Win32::Foundation::{FALSE, TRUE};
use windows::Win32::Graphics::Direct3D11::{D3D11_BLEND, D3D11_BLEND_DESC, D3D11_BLEND_ONE, D3D11_BLEND_OP_ADD, D3D11_BLEND_ZERO, D3D11_COLOR_WRITE_ENABLE_ALL, D3D11_RENDER_TARGET_BLEND_DESC, ID3D11BlendState, ID3D11Device, ID3D11Resource, ID3D11ShaderResourceView};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

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
    Ok(unsafe {
        let mut blend_state = std::mem::zeroed();
        device.CreateBlendState(&D3D11_BLEND_DESC {
            RenderTarget: [D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: TRUE,
                SrcBlend: src,
                DestBlend: dst,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ONE,
                DestBlendAlpha: D3D11_BLEND_ZERO,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as _,
            }; 8],
            IndependentBlendEnable: FALSE,
            AlphaToCoverageEnable: FALSE
        }, Some(&mut blend_state))?;
        blend_state.some()?
    })
}

pub fn make_shader_resource_view<T: Into<InParam<ID3D11Resource>>>(device: &ID3D11Device, resource: T) -> Result<ID3D11ShaderResourceView> {
    Ok(unsafe {
        let mut view = std::mem::zeroed();
        device.CreateShaderResourceView(resource, None, Some(&mut view))?;
        view.some()?
    })
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

