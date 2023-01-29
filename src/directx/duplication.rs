use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::{DXGI_OUTDUPL_POINTER_SHAPE_TYPE, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR, DXGI_OUTDUPL_POINTER_SHAPE_INFO, IDXGIOutputDuplication, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_ACCESS_DENIED, DXGI_ERROR_INVALID_CALL, DXGI_ERROR_WAIT_TIMEOUT};
use windows::core::Interface;
use windows::Win32::Foundation::{POINT, GetLastError};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT};
use windows::Win32::System::StationsAndDesktops::{OpenInputDesktop, SetThreadDesktop, DF_ALLOWOTHERACCOUNTHOOK, DESKTOP_ACCESS_FLAGS};
use anyhow::Result;
use windows::Win32::System::SystemServices::GENERIC_READ;
use crate::directx::{Display, DisplayMode};

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct AcquisitionResults {
    pub success: bool,
    pub cursor_updated: bool
}

pub struct DesktopDuplication {
    d3d_device: ID3D11Device,
    output: Display,
    display_mode: DisplayMode,
    dupl: Option<IDXGIOutputDuplication>,
    frame: Option<ID3D11Texture2D>,
    cursor_pos: Option<POINT>,
    cursor_data: Option<CursorData>,
}

impl DesktopDuplication {
    pub fn new(d3d_device: &ID3D11Device, output: Display) -> Result<Self> {
        let dupl = Self::create_dupl_output(&d3d_device, &output)?;
        let display_mode = output.get_current_display_mode()?;
        Ok(Self {
            d3d_device: d3d_device.clone(),
            output,
            display_mode,
            dupl: Some(dupl),
            frame: None,
            cursor_pos: None,
            cursor_data: None,
        })
    }

    fn create_dupl_output(device: &ID3D11Device, output: &Display) -> Result<IDXGIOutputDuplication> {
        let supported_formats = [DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT];
        let dupl = unsafe { output.as_raw_ref().DuplicateOutput1(device, 0, &supported_formats)? };
        Ok(dupl)
    }

    pub fn try_acquire_next_frame(&mut self) -> Result<AcquisitionResults> {
        let mut result = Default::default();
        let mut frame_info = Default::default();
        self.release_locked_frame();
        if self.dupl.is_none() {
            self.reacquire_dup()?;
        }
        let dupl = self.dupl.as_ref().unwrap();
        let mut resource = unsafe {std::mem::zeroed()};
        let status = unsafe { dupl.AcquireNextFrame(0, &mut frame_info, &mut resource) };
        if let Err(e) = status {
            return match e.code() {
                DXGI_ERROR_ACCESS_LOST | DXGI_ERROR_ACCESS_DENIED | DXGI_ERROR_INVALID_CALL => {
                    log::warn!("trying to reaquire duplication instance");
                    self.reacquire_dup()?;
                    Err(e.into())
                }
                DXGI_ERROR_WAIT_TIMEOUT => Ok(result),
                _ => Err(e.into())
            }
        }

        match resource {
            Some(resource) => self.frame = Some(resource.cast().unwrap()),
            None => return Err(anyhow::anyhow!("Resource is null"))
        }
        result.success = true;

        if frame_info.PointerShapeBufferSize != 0 {
            let cursor_data = self.cursor_data.get_or_insert_with(|| CursorData {
                cursor_type: CursorType::Color,
                width: 0,
                height: 0,
                data: vec![0u8; frame_info.PointerShapeBufferSize as usize],
            });
            cursor_data.data.resize(frame_info.PointerShapeBufferSize as usize, 0u8);
            let mut used_size = 0;
            let mut info: DXGI_OUTDUPL_POINTER_SHAPE_INFO = Default::default();
            unsafe {
                dupl.GetFramePointerShape(
                    cursor_data.data.len() as u32,
                    cursor_data.data.as_mut_ptr() as _,
                    &mut used_size,
                    &mut info).unwrap();
            }
            cursor_data.cursor_type = info.Type.into();
            cursor_data.width = info.Width;
            cursor_data.height = match cursor_data.cursor_type {
                CursorType::Monochrome => info.Height / 2,
                _ => info.Height
            };
            result.cursor_updated = true;
        }

        if frame_info.LastMouseUpdateTime != 0 {
            self.cursor_pos = if frame_info.PointerPosition.Visible.as_bool() {
                Some(frame_info.PointerPosition.Position)
            } else {
                None
            }
        }

        Ok(result)
    }

    pub fn get_frame(&self) -> Option<&ID3D11Texture2D> {
        self.frame.as_ref()
    }

    pub fn get_cursor_pos(&self) -> Option<POINT> {
        self.cursor_pos
    }

    pub fn get_cursor_data(&self) -> Option<&CursorData> {
        self.cursor_data.as_ref()
    }

    pub fn get_display_mode(&self) -> DisplayMode {
        self.display_mode
    }

    pub fn get_current_output(&self) -> &Display {
        &self.output
    }

    fn reacquire_dup(&mut self) -> Result<()> {
        self.dupl = None;
        self.release_locked_frame();

        let dupl = Self::create_dupl_output(&self.d3d_device, &self.output);
        if dupl.is_err() {
            let _ = Self::switch_thread_desktop();
        }
        let dupl = dupl?;
        log::trace!("successfully acquired new duplication instance");
        self.dupl = Some(dupl);
        self.display_mode = self.output.get_current_display_mode()?;
        Ok(())
    }

    fn release_locked_frame(&mut self) {
        self.frame = None;
        if self.dupl.is_some() {
            let _ = unsafe { self.dupl.as_ref().unwrap().ReleaseFrame() };
        }
    }

    fn switch_thread_desktop() -> Result<()> {
        log::trace!("trying to switch Thread desktop");
        let desk = unsafe { OpenInputDesktop(DF_ALLOWOTHERACCOUNTHOOK as _, true, DESKTOP_ACCESS_FLAGS(GENERIC_READ)) };
        if let Err(err) = desk {
            log::error!("didnt get desktop : {:?}", err);
            return Err(anyhow::anyhow!("AccessDenied"));
        }
        let result = unsafe { SetThreadDesktop(desk.unwrap()) };
        if !result.as_bool() {
            log::error!("didnt switch desktop: {:?}",unsafe{GetLastError().to_hresult()});
            return Err(anyhow::anyhow!("AccessDenied"));
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CursorData {
    pub cursor_type: CursorType,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>
}
