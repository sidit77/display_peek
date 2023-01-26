use windows::core::HSTRING;
use anyhow::Result;
use windows::Win32::Foundation::{FALSE, TRUE};
use windows::Win32::Graphics::Direct3D11::{D3D11_BLEND, D3D11_BLEND_DESC, D3D11_BLEND_ONE, D3D11_BLEND_OP_ADD, D3D11_BLEND_ZERO, D3D11_COLOR_WRITE_ENABLE_ALL, D3D11_RENDER_TARGET_BLEND_DESC, ID3D11BlendState, ID3D11Device};

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
    unsafe {
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
        Ok(blend_state.unwrap())
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

impl ExactSizeIterator for U8Iter {

}