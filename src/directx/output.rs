#![allow(dead_code)]

use std::cmp::max;
use std::ffi::{CString};
use std::mem::{size_of};
use std::ptr::{null, null_mut};
use windows::Win32::Graphics::Dxgi::{DXGI_MODE_DESC1, IDXGIOutput6};
use windows::core::{PCSTR};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R8G8B8A8_UNORM};
use windows::Win32::Graphics::Gdi::{CDS_TYPE, ChangeDisplaySettingsExA, DEVMODE_DISPLAY_ORIENTATION, DEVMODEA, DISP_CHANGE_SUCCESSFUL, DM_BITSPERPEL, DM_DISPLAYFREQUENCY, DM_DISPLAYORIENTATION, DM_PELSHEIGHT, DM_PELSWIDTH, DMDO_180, DMDO_270, DMDO_90, DMDO_DEFAULT, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsExA, HMONITOR};
use anyhow::{anyhow, Context, Result};
use glam::{Mat4, Quat, vec3};
use crate::utils::convert_u16_to_string;

#[repr(transparent)]
#[derive(Clone)]
pub struct Display(IDXGIOutput6);

impl PartialEq<Self> for Display {
    fn eq(&self, other: &Self) -> bool {
        self.hmonitor().ok() == other.hmonitor().ok()
    }
}

impl Eq for Display {}

impl Display {
    pub fn new(output: IDXGIOutput6) -> Self {
        Self(output)
    }

    pub fn name(&self) -> Result<String> {
        let mut desc = Default::default();
        unsafe { self.0.GetDesc1(&mut desc)? };
        Ok(convert_u16_to_string(&desc.DeviceName))
    }

    pub fn hmonitor(&self) -> Result<HMONITOR> {
        let mut desc = Default::default();
        unsafe { self.0.GetDesc1(&mut desc)? };
        Ok(desc.Monitor)
    }

    pub fn get_display_modes(&self) -> Result<Vec<DisplayMode>> {
        let mut out = Vec::new();
        self.fill_modes(DXGI_FORMAT_R8G8B8A8_UNORM, false, &mut out)?;
        self.fill_modes(DXGI_FORMAT_R16G16B16A16_FLOAT, true, &mut out)?;
        Ok(out)
    }

    pub fn set_display_mode(&self, mode: &DisplayMode) -> Result<()> {
        let name = self.name()?;
        let name = CString::new(name)?;
        let mut display_mode = DEVMODEA {
            ..Default::default()
        };
        display_mode.dmSize = size_of::<DEVMODEA>() as _;
        match mode.orientation {
            DisplayOrientation::Landscape | DisplayOrientation::FlippedLandscape => {
                display_mode.dmPelsHeight = mode.height;
                display_mode.dmPelsWidth = mode.width;
            }
            DisplayOrientation::Portrait | DisplayOrientation::FlippedPortrait => {
                display_mode.dmPelsHeight = mode.width;
                display_mode.dmPelsWidth = mode.height;
            }
        }
        display_mode.dmBitsPerPel = if mode.hdr { 64 } else { 32 };
        display_mode.dmDisplayFrequency = mode.refresh_num / mode.refresh_den;
        display_mode.Anonymous1.Anonymous2.dmDisplayOrientation = mode.orientation.into();

        display_mode.dmFields |= DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY | DM_BITSPERPEL | DM_DISPLAYORIENTATION;

        let resp = unsafe { ChangeDisplaySettingsExA(PCSTR(name.as_ptr() as _), Some(&display_mode), None, CDS_TYPE(0), Some(null())) };

        if resp != DISP_CHANGE_SUCCESSFUL {
            Err(anyhow!("failed to change display settings. DISP_CHANGE={}", resp.0))
        } else {
            Ok(())
        }
    }

    pub fn get_current_display_mode(&self) -> Result<DisplayMode> {
        let name = self.name()?;
        let name = CString::new(name)?;

        let mut mode: DEVMODEA = DEVMODEA {
            dmSize: size_of::<DEVMODEA>() as _,
            dmDriverExtra: 0,
            ..Default::default()
        };
        let success = unsafe { EnumDisplaySettingsExA(PCSTR(name.as_c_str().as_ptr() as _), ENUM_CURRENT_SETTINGS, &mut mode, 0) };
        if !success.as_bool() {
            Err(anyhow!("Failed to retrieve display settings for output"))
        } else {
            let mut dm = DisplayMode {
                width: mode.dmPelsWidth,
                height: mode.dmPelsHeight,
                orientation: unsafe { mode.Anonymous1.Anonymous2.dmDisplayOrientation }.into(),
                refresh_num: mode.dmDisplayFrequency,
                refresh_den: 1,
                hdr: mode.dmBitsPerPel != 32,
            };
            if matches!(dm.orientation,DisplayOrientation::Portrait|DisplayOrientation::FlippedPortrait) {
                dm.height = mode.dmPelsWidth;
                dm.width = mode.dmPelsHeight;
            }
            Ok(dm)
        }
    }

    pub fn wait_for_vsync(&self) -> Result<()> {
        unsafe { self.0.WaitForVBlank().context("DisplaySyncStream received a sync error. Maybe monitor disconnected?") }
    }

    pub fn as_raw_ref(&self) -> &IDXGIOutput6 {
        &self.0
    }

    fn fill_modes(&self, format: DXGI_FORMAT, hdr: bool, mode_list: &mut Vec<DisplayMode>) -> Result<()> {
        let mut num_modes: u32 = 0;
        if let Err(e) = unsafe { self.0.GetDisplayModeList1(format, 0, &mut num_modes, Some(null_mut())) } {
            return Err(anyhow!("{:?}", e));
        }

        let mut modes: Vec<DXGI_MODE_DESC1> = Vec::with_capacity(num_modes as _);
        if let Err(e) = unsafe { self.0.GetDisplayModeList1(format, 0, &mut num_modes, Some(modes.as_mut_ptr())) } {
            return Err(anyhow!("{:?}", e));
        }

        unsafe { modes.set_len(num_modes as _) };
        let reserve = max(0, num_modes as usize - mode_list.capacity() + mode_list.len());
        mode_list.reserve(reserve);
        for mode in modes.iter() {
            mode_list.push(DisplayMode {
                width: mode.Width,
                height: mode.Height,
                refresh_num: mode.RefreshRate.Numerator,
                refresh_den: mode.RefreshRate.Denominator,
                hdr,
                ..Default::default()
            })
        }
        Ok(())
    }
}

unsafe impl Send for Display {}

unsafe impl Sync for Display {}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default)]
pub enum DisplayOrientation {
    #[default]
    Landscape,
    Portrait,
    FlippedLandscape,
    FlippedPortrait,
}

impl From<DEVMODE_DISPLAY_ORIENTATION> for DisplayOrientation {
    fn from(i: DEVMODE_DISPLAY_ORIENTATION) -> Self {
        match i {
            DMDO_90 => Self::Portrait,
            DMDO_180 => Self::FlippedLandscape,
            DMDO_270 => Self::FlippedPortrait,
            _ => Self::Landscape,
        }
    }
}

impl From<DisplayOrientation> for DEVMODE_DISPLAY_ORIENTATION {
    fn from(i: DisplayOrientation) -> Self {
        match i {
            DisplayOrientation::Landscape => { DMDO_DEFAULT }
            DisplayOrientation::Portrait => { DMDO_90 }
            DisplayOrientation::FlippedLandscape => { DMDO_180 }
            DisplayOrientation::FlippedPortrait => { DMDO_270 }
        }
    }
}


#[repr(C)]
#[derive(Copy, Clone, Default, Debug)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub orientation: DisplayOrientation,
    pub refresh_num: u32,
    pub refresh_den: u32,
    pub hdr: bool,
}

impl DisplayMode {

    pub fn get_flipped_size(self) -> (u32, u32) {
        match self.orientation {
            DisplayOrientation::Landscape | DisplayOrientation::FlippedLandscape => (self.width, self.height),
            DisplayOrientation::FlippedPortrait | DisplayOrientation::Portrait => (self.height, self.width)
        }
    }

    pub fn get_frame_transform(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            vec3(self.width as f32, self.height as f32, 0.0),
            match self.orientation {
                DisplayOrientation::Landscape => Quat::from_rotation_z(0f32.to_radians()),
                DisplayOrientation::Portrait => Quat::from_rotation_z(90f32.to_radians()),
                DisplayOrientation::FlippedLandscape => Quat::from_rotation_z(180f32.to_radians()),
                DisplayOrientation::FlippedPortrait => Quat::from_rotation_z(270f32.to_radians()),
            },
            match self.orientation {
                DisplayOrientation::Landscape => vec3(0.0, 0.0, 0.0),
                DisplayOrientation::Portrait => vec3(self.height as f32, 0.0, 0.0),
                DisplayOrientation::FlippedLandscape => vec3(self.width as f32, self.height as f32, 0.0),
                DisplayOrientation::FlippedPortrait => vec3(0.0, self.width as f32, 0.0),
            },
        )
    }

}