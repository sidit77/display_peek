use std::ffi::c_void;
use std::mem::size_of;
use anyhow::Result;
use error_tools::SomeOptionExt;
use windows::Win32::Graphics::Direct3D11::{D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_FLAG, D3D11_RESOURCE_MISC_GENERATE_MIPS, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, ID3D11Device, ID3D11DeviceContext4, ID3D11ShaderResourceView, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};
use crate::directx::{CursorData, CursorType};
use crate::utils::U8Iter;

pub struct CursorSprite {
    pub valid: bool,
    pub cursor_type: CursorType,
    pub width: u32,
    pub height: u32,
    pub norm: (ID3D11Texture2D, ID3D11ShaderResourceView),
    pub mask: (ID3D11Texture2D, ID3D11ShaderResourceView),
}

impl CursorSprite {

    pub fn norm_srv(&self) -> &ID3D11ShaderResourceView {
        debug_assert!(self.valid);
        &self.norm.1
    }

    pub fn mask_srv(&self) -> &ID3D11ShaderResourceView {
        debug_assert!(self.valid);
        debug_assert!(matches!(self.cursor_type, CursorType::MaskedColor | CursorType::Monochrome));
        &self.mask.1
    }

    pub fn new(device: &ID3D11Device, width: u32, height: u32) -> Result<Self> {
        log::trace!("Allocating new cursor textures: {}x{}", width, height);
        Ok(Self {
            valid: false,
            cursor_type: CursorType::Color,
            width,
            height,
            norm: make_texture(device, width, height)?,
            mask: make_texture(device, width, height)?,
        })
    }

    pub fn update(&mut self, device: &ID3D11Device, context: &ID3D11DeviceContext4, data: &CursorData) -> Result<()> {
        self.resize(device, data.width, data.height)?;
        self.update_content(context, data.cursor_type, data.data.as_slice());
        Ok(())
    }

    fn update_content(&mut self, context: &ID3D11DeviceContext4, cursor_type: CursorType, data: &[u8]) {
        self.cursor_type = cursor_type;
        match cursor_type {
            CursorType::Monochrome => {
                assert_eq!((2 * self.height * self.width / u8::BITS) as usize, data.len());
                let (and_mask, xor_mask) = data.split_at((self.height * self.width / u8::BITS) as usize);
                assert_eq!(and_mask.len(), xor_mask.len());
                let and_buffer: Vec<u32> = and_mask
                    .iter()
                    .flat_map(|mask|U8Iter::new(*mask))
                    .map(|b | if b {0xFFFFFFFF} else {0xFF000000})
                    .collect();

                let xor_buffer: Vec<u32> = xor_mask
                    .iter()
                    .flat_map(|mask|U8Iter::new(*mask))
                    .map(|b | if b {0x00FFFFFF} else {0x00000000})
                    .collect();

                self.update_textures(context, Some(and_buffer.as_ptr() as _), Some(xor_buffer.as_ptr() as _));
            },
            CursorType::Color => {
                assert_eq!((self.height * self.width) as usize * size_of::<u32>(), data.len());
                let buffer: Vec<u32> = bgra_to_rgba(data)
                    .collect();
                self.update_textures(context, Some(buffer.as_ptr() as _), None)
            },
            CursorType::MaskedColor => {
                assert_eq!((self.height * self.width) as usize * size_of::<u32>(), data.len());
                let (color_buffer, xor_buffer): (Vec<u32>, Vec<u32>) = bgra_to_rgba(data)
                    .map(|c| match (c & 0xFF000000) != 0 {
                        true => (c & 0x00FFFFFF, c & 0x00FFFFFF),
                        false => (c | 0xFF000000, 0xFF000000)
                    })
                    .unzip();

                self.update_textures(context, Some(color_buffer.as_ptr() as _), Some(xor_buffer.as_ptr() as _));
            }
        }
        self.valid = true;
    }

    fn resize(&mut self, device: &ID3D11Device, width: u32, height: u32) -> Result<()> {
        if self.width != width || self.height != height {
            *self = Self::new(device, width, height)?;
        }
        Ok(())
    }

    fn update_textures(&self, context: &ID3D11DeviceContext4, norm: Option<*const c_void>, mask: Option<*const c_void>){
        unsafe {
            let row_pitch = size_of::<u32>() as u32 * self.width;
            let depth_pitch = row_pitch * self.height;
            if let Some(buf) = norm {
                let (tex, srv) = &self.norm;
                context.UpdateSubresource(tex, 0, None, buf, row_pitch,depth_pitch);
                context.GenerateMips(srv);
            }
            if let Some(buf) = mask {
                let (tex, srv) = &self.mask;
                context.UpdateSubresource(tex, 0, None, buf, row_pitch,depth_pitch);
                context.GenerateMips(srv);
            }
        }
    }

}

fn bgra_to_rgba(bytes: &[u8]) -> impl Iterator<Item=u32> + '_ {
    bytes
        .chunks_exact(size_of::<u32>())
        .map(|b| u32::from_le_bytes([b[2], b[1], b[0], b[3]]))
}

fn make_texture(device: &ID3D11Device, width: u32, height: u32) -> Result<(ID3D11Texture2D, ID3D11ShaderResourceView)> {
    let tex = unsafe {
        let mut tex = std::mem::zeroed();
        device.CreateTexture2D(&D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 0,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
            CPUAccessFlags: D3D11_CPU_ACCESS_FLAG(0),
            MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS,
        }, None, Some(&mut tex))?;
        tex.some()?
    };
    let srv = unsafe {
        let mut srv = std::mem::zeroed();
        device.CreateShaderResourceView(&tex, None, Some(&mut srv))?;
        srv.some()?
    };
    Ok((tex, srv))
}