use std::ffi::c_void;
use std::mem::size_of;
use log::{debug, error, trace, warn};
use windows::Win32::Graphics::Direct3D11::{D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_FLAG, D3D11_RESOURCE_MISC_FLAG, D3D11_RESOURCE_MISC_GENERATE_MIPS, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, ID3D11Device, ID3D11Device4, ID3D11DeviceContext4, ID3D11ShaderResourceView, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::{DXGI_OUTDUPL_POINTER_SHAPE_TYPE, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR, DXGI_OUTDUPL_POINTER_SHAPE_INFO, DXGI_ERROR_UNSUPPORTED, DXGI_ERROR_SESSION_DISCONNECTED, IDXGIDevice4, IDXGIOutputDuplication, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_ACCESS_DENIED, DXGI_ERROR_INVALID_CALL, DXGI_ERROR_WAIT_TIMEOUT, IDXGIResource};
use windows::core::Interface;
use windows::core::Result as WinResult;
use windows::Win32::Foundation::{E_INVALIDARG, E_ACCESSDENIED, POINT, GetLastError};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::System::StationsAndDesktops::{OpenInputDesktop, SetThreadDesktop, DF_ALLOWOTHERACCOUNTHOOK, DESKTOP_ACCESS_FLAGS};
use anyhow::{anyhow, Result};
use windows::Win32::System::SystemServices::GENERIC_READ;
use crate::directx::Display;
use crate::utils::U8Iter;

pub struct DesktopDuplicationApi {
    d3d_device: ID3D11Device4,
    d3d_ctx: ID3D11DeviceContext4,
    //d2d_ctx: ID2D1DeviceContext5,
    output: Display,
    dupl: Option<IDXGIOutputDuplication>,

    options: DuplicationApiOptions,

    state: DuplicationState,

}

unsafe impl Send for DesktopDuplicationApi {}

unsafe impl Sync for DesktopDuplicationApi {}


impl DesktopDuplicationApi {
    pub fn new_with(d3d_device: ID3D11Device, d3d_ctx: ID3D11DeviceContext4, output: Display) -> Result<Self> {
        let d3d_device = d3d_device.cast()?;
        let dupl = Self::create_dupl_output(&d3d_device, &output)?;
        Ok(Self {
            d3d_device,
            d3d_ctx,
            //d2d_ctx,
            output,
            dupl: Some(dupl),
            options: Default::default(),
            state: Default::default(),
        })
    }

    fn create_dupl_output(dev: &ID3D11Device4, output: &Display) -> Result<IDXGIOutputDuplication> {
        let supported_formats = [DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT];
        let device: IDXGIDevice4 = dev.cast().unwrap();
        let dupl: WinResult<IDXGIOutputDuplication> = unsafe { output.as_raw_ref().DuplicateOutput1(&device, 0, &supported_formats) };

        if let Err(err) = dupl {
            return match err.code() {
                E_INVALIDARG => {
                    Err(anyhow!("failed to create duplicate output. {:?}", err))
                }
                E_ACCESSDENIED => {
                    Err(anyhow!("DDApiError::AccessDenied"))
                }
                DXGI_ERROR_UNSUPPORTED => {
                    Err(anyhow!("DDApiError::Unsupported"))
                }
                DXGI_ERROR_SESSION_DISCONNECTED => {
                    Err(anyhow!("DDApiError::Disconnected"))
                }
                _ => {
                    Err(anyhow!("DDApiError::Unexpected(err.to_string())"))
                }
            };
        }
        Ok(dupl.unwrap())
    }

    /// unlike [acquire_next_vsync_frame][Self::acquire_next_vsync_frame], this is a blocking call and immediately returns the texture
    /// without waiting for vsync.
    ///
    /// the method handles any switches in desktop automatically.
    ///
    /// this fails with following results:
    ///
    /// ## Recoverable errors
    /// these can be recovered by just calling the function again after this error.
    /// * [DDApiError::AccessLost] - when desktop mode switch happens (resolution change) or desktop
    /// changes. (going to lock screen etc).
    /// * [DDApiError::AccessDenied] - when windows opens a secure environment, this application
    /// will be denied access.
    ///
    /// ## Non-recoverable errors
    /// * [DDApiError::Unexpected] - this type of error cant be recovered from. the application should
    /// drop the struct and re create a new instance.
    pub fn try_acquire_next_frame(&mut self) -> Result<()> {
        let mut frame_info = Default::default();
        self.release_locked_frame();
        if self.dupl.is_none() {
            self.reacquire_dup()?;
        }
        let dupl = self.dupl.as_ref().unwrap();
        let status = unsafe { dupl.AcquireNextFrame(0, &mut frame_info, &mut self.state.last_resource) };
        if let Err(e) = status {
            match e.code() {
                DXGI_ERROR_ACCESS_LOST => {
                    warn!("display access lost. maybe desktop mode switch?, {:?}",e);
                    self.reacquire_dup()?;
                    //return Err(DDApiError::AccessLost);
                    return Err(anyhow::anyhow!("Error"));
                }
                DXGI_ERROR_ACCESS_DENIED => {
                    warn!("display access is denied. Maybe running in a secure environment?");
                    self.reacquire_dup()?;
                    //return Err(DDApiError::AccessDenied);
                    return Err(anyhow::anyhow!("Error"));
                }
                DXGI_ERROR_INVALID_CALL => {
                    warn!("dxgi_error_invalid_call. maybe forgot to ReleaseFrame()?");
                    self.reacquire_dup()?;
                    //return Err(DDApiError::AccessLost);
                    return Err(anyhow::anyhow!("Error"));
                }
                DXGI_ERROR_WAIT_TIMEOUT => {
                    //trace!("no new frame is available");
                }
                _ => {
                    //return Err(DDApiError::Unexpected(format!("acquire frame failed {:?}", e)));
                    return Err(anyhow::anyhow!("Error"));
                }
            }
        }

        match self.state.last_resource.as_ref() {
            Some(resource) => self.state.frame = Some(resource.cast().unwrap()),
            None => return Err(anyhow::anyhow!("Error"))//return Err(DDApiError::Unexpected(String::from("Resource was none")))
        }



        //let new_frame = Texture::new(resource.cast().unwrap());
        //self.ensure_cache_frame(&new_frame)?;
        //unsafe { self.d3d_ctx.CopyResource(self.state.frame.as_ref().unwrap().as_raw_ref(), new_frame.as_raw_ref()); }

        //log::trace!("{:#?}", frame_info);

        if frame_info.PointerShapeBufferSize != 0 {
            let mut used_size = 0;
            let mut info: DXGI_OUTDUPL_POINTER_SHAPE_INFO = Default::default();
            let mut cursor_buffer = vec![0u8; frame_info.PointerShapeBufferSize as usize];
            unsafe {
                dupl.GetFramePointerShape(
                    cursor_buffer.len() as u32,
                    cursor_buffer.as_mut_ptr() as _,
                    &mut used_size,
                    &mut info).unwrap();

            }
            let cursor_type: CursorType = info.Type.into();
            let width = info.Width;
            let height = match cursor_type {
                CursorType::Monochrome => info.Height / 2,
                _ => info.Height
            };
            let cursor = self.state.cursor_bitmap.get_or_insert_with(|| Cursor::new(&self.d3d_device, cursor_type, width, height));
            cursor.resize(&self.d3d_device, cursor_type, width, height);
            cursor.update_content(&self.d3d_ctx, cursor_buffer.as_slice());
        }

        if frame_info.LastMouseUpdateTime != 0 {
            self.state.cursor_pos = if frame_info.PointerPosition.Visible.as_bool() {
                Some(frame_info.PointerPosition.Position)
            } else {
                None
            }
        }

        Ok(())
        //if self.state.frame.is_none() {
        //    return Err(DDApiError::AccessLost);
        //}
//
        //let cache_cursor_frame = self.state.frame.clone().unwrap();
//
        //if !self.options.skip_cursor {
        //    //self.draw_cursor(&cache_cursor_frame)?
        //}
        //Ok(cache_cursor_frame)
    }

    pub fn get_frame(&self) -> Option<&ID3D11Texture2D> {
        self.state.frame.as_ref()
    }

    pub fn get_cursor(&self) -> Option<(POINT, &Cursor)> {
        self.state.cursor_pos.zip(self.state.cursor_bitmap.as_ref())
    }

    /// this method is used to retrieve device and context used in this api. These can be used
    /// to build directx color conversion and image scale.
    pub fn get_device_and_ctx(&self) -> (ID3D11Device4, ID3D11DeviceContext4) {
        return (self.d3d_device.clone(), self.d3d_ctx.clone());
    }

    /// configure duplication manager with given options.
    pub fn configure(&mut self, opt: DuplicationApiOptions) {
        self.options = opt;
    }

    pub fn switch_output(&mut self, display: Display) -> Result<()>{
        self.output = display;
        self.reacquire_dup()?;
        Ok(())
    }

    fn reacquire_dup(&mut self) -> Result<()> {
        self.state.reset();
        self.dupl = None;

        let dupl = Self::create_dupl_output(&self.d3d_device, &self.output);
        if dupl.is_err() {
            let _ = Self::switch_thread_desktop();
        }
        let dupl = dupl?;
        debug!("successfully acquired new duplication instance");
        self.dupl = Some(dupl);
        Ok(())
    }

    fn release_locked_frame(&mut self) {
        self.state.frame = None;
        self.state.last_resource = None;
        if self.dupl.is_some() {
            let _ = unsafe { self.dupl.as_ref().unwrap().ReleaseFrame() };
        }
    }

    /*
    fn create_cursor_bitmap(&self, width: u32, height: u32, ptr: *const core::ffi::c_void) -> ID2D1Bitmap {
        unsafe {
            self.d2d_ctx.CreateBitmap(
                D2D_SIZE_U {
                    width,
                    height,
                },
                Some(ptr),
                width * 4,
                &D2D1_BITMAP_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM ,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                }
            ).unwrap()
        }

    }

     */
    /*
    fn ensure_cache_frame(&mut self, frame: &Texture) -> Result<()> {
        if self.state.frame.is_none() {
            let tex = self.create_texture(frame.desc(), D3D11_USAGE_DEFAULT,
                                          D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
                                          D3D11_RESOURCE_MISC_GDI_COMPATIBLE)?;
            self.state.frame = Some(tex);
        }
        Ok(())
    }

    fn create_texture(&self, tex_desc: TextureDesc, usage: D3D11_USAGE, bind_flags: D3D11_BIND_FLAG,
                      misc_flag: D3D11_RESOURCE_MISC_FLAG) -> Result<Texture> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: tex_desc.width,
            Height: tex_desc.height,
            MipLevels: 1,
            ArraySize: 1,
            Format: tex_desc.format.into(),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: usage,
            BindFlags: bind_flags,
            CPUAccessFlags: Default::default(),
            MiscFlags: misc_flag,
        };
        let result = unsafe { self.d3d_device.CreateTexture2D(&desc, Some(null())) };
        if let Err(e) = result {
            Err(DDApiError::Unexpected(format!("failed to create texture. {:?}", e)))
        } else {
            Ok(Texture::new(result.unwrap()))
        }
    }
 */
    fn switch_thread_desktop() -> Result<()> {
        debug!("trying to switch Thread desktop");
        let desk = unsafe { OpenInputDesktop(DF_ALLOWOTHERACCOUNTHOOK as _, true, DESKTOP_ACCESS_FLAGS(GENERIC_READ)) };
        if let Err(err) = desk {
            error!("dint get desktop : {:?}", err);
            //return Err(DDApiError::AccessDenied);
            return Err(anyhow::anyhow!("Error"));
        }
        let result = unsafe { SetThreadDesktop(desk.unwrap()) };
        if !result.as_bool() {
            error!("dint switch desktop: {:?}",unsafe{GetLastError().to_hresult()});
            //return Err(DDApiError::AccessDenied);
            return Err(anyhow::anyhow!("Error"));
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CursorType {
    Color,
    Monochrome,
    MaskedColor
}

impl From<u32> for CursorType {
    fn from(value: u32) -> Self {
        match DXGI_OUTDUPL_POINTER_SHAPE_TYPE(value as i32) {
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME => CursorType::Monochrome,
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR => CursorType::Color,
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR => CursorType::MaskedColor,
            _ => unreachable!()
        }
    }
}

pub struct Cursor {
    pub cursor_type: CursorType,
    pub width: u32,
    pub height: u32,
    pub norm: (ID3D11Texture2D, ID3D11ShaderResourceView),
    pub mask: (ID3D11Texture2D, ID3D11ShaderResourceView),
}

impl Cursor {

    pub fn norm_srv(&self) -> &ID3D11ShaderResourceView {
        &self.norm.1
    }

    pub fn mask_srv(&self) -> &ID3D11ShaderResourceView {
        &self.mask.1
    }

    fn new(device: &ID3D11Device4, cursor_type: CursorType, width: u32, height: u32) -> Self {
        log::trace!("Allocating new cursor textures: {}x{}", width, height);
        Self {
            cursor_type,
            width,
            height,
            norm: make_texture(device, width, height),
            mask: make_texture(device, width, height),
        }
    }

    fn update_content(&mut self, context: &ID3D11DeviceContext4, data: &[u8]) {
        match self.cursor_type {
            CursorType::Monochrome => {
                let (and_mask, xor_mask) = data.split_at((self.height * self.width / u8::BITS) as usize);
                assert_eq!(and_mask.len(), xor_mask.len());
                let and_buffer: Vec<u32> = and_mask
                    .into_iter()
                    .flat_map(|mask|U8Iter::new(*mask))
                    .map(|b | if b {0xFFFFFFFF} else {0xFF000000})
                    .collect();

                let xor_buffer: Vec<u32> = xor_mask
                    .into_iter()
                    .flat_map(|mask|U8Iter::new(*mask))
                    .map(|b | if b {0x00FFFFFF} else {0x00000000})
                    .collect();

                self.update_textures(context, Some(and_buffer.as_ptr() as _), Some(xor_buffer.as_ptr() as _));
            },
            CursorType::Color => {
                self.update_textures(context, Some(data.as_ptr() as _), None)
            },
            CursorType::MaskedColor => {
                let (color_buffer, xor_buffer): (Vec<u32>, Vec<u32>) = data
                    .chunks_exact(4)
                    .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                    .map(|c| match (c & 0xFF000000) != 0 {
                        //wfe
                        true => (c & 0x00FFFFFF, c & 0x00FFFFFF),
                        false => (c | 0xFF000000, 0xFF000000)
                    })
                    .unzip();

                self.update_textures(context, Some(color_buffer.as_ptr() as _), Some(xor_buffer.as_ptr() as _));
            }
        }
    }

    fn resize(&mut self, device: &ID3D11Device4, cursor_type: CursorType, width: u32, height: u32) {
        self.cursor_type = cursor_type;
        if self.width != width || self.height != height {
            *self = Self::new(device, cursor_type, width, height);
        }
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

fn make_texture(device: &ID3D11Device4, width: u32, height: u32) -> (ID3D11Texture2D, ID3D11ShaderResourceView) {
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
        }, None, Some(&mut tex)).unwrap();
        tex.unwrap()
    };
    let srv = unsafe {
        let mut srv = std::mem::zeroed();
        device.CreateShaderResourceView(&tex, None, Some(&mut srv)).unwrap();
        srv.unwrap()
    };
    (tex, srv)
}

/// Settings to configure Desktop duplication api. these can be configured even after initialized.
///
/// currently it only supports option to skip drawing cursor
#[derive(Default)]
pub struct DuplicationApiOptions {
    pub skip_cursor: bool,
}

// these are state variables for duplication sync stream
#[derive(Default)]
struct DuplicationState {
    last_resource: Option<IDXGIResource>,
    frame: Option<ID3D11Texture2D>,
    cursor_pos: Option<POINT>,
    cursor_bitmap: Option<Cursor>,
}

impl DuplicationState {
    pub fn reset(&mut self) {
        self.last_resource = None;
        self.frame = None;
        self.cursor_pos = None;
        self.cursor_bitmap = None;
    }
}