#![allow(dead_code)]

use windows::core::{Interface};
use windows::Win32::Foundation::LUID;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory2, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, IDXGIAdapter4, IDXGIFactory6};
use anyhow::Result;
use crate::directx::Display;

use crate::utils::convert_u16_to_string;

#[repr(transparent)]
#[derive(Clone)]
pub struct Adapter(IDXGIAdapter4);

unsafe impl Send for Adapter {}

unsafe impl Sync for Adapter {}

impl Adapter {
    pub fn name(&self) -> Result<String> {
        let mut desc = Default::default();
        unsafe { self.0.GetDesc3(&mut desc)? };
        Ok(convert_u16_to_string(&desc.Description))
    }

    pub fn luid(&self) -> Result<LUID> {
        let mut desc = Default::default();
        unsafe { self.0.GetDesc3(&mut desc)? };
        Ok(desc.AdapterLuid)
    }

    pub fn as_raw_ref(&self) -> &IDXGIAdapter4 {
        &self.0
    }

    pub fn iter_displays(&self) -> DisplayIterator {
        DisplayIterator::new(self.clone())
    }

    pub fn get_display_by_idx(&self, idx: u32) -> Option<Display> {
        DisplayIterator::get_display_by_idx(self, idx)
    }
}

#[repr(C)]
#[derive(Clone)]
pub struct DisplayIterator {
    adapter: Adapter,
    idx: u32,
}

impl DisplayIterator {
    fn new(adapter: Adapter) -> Self {
        Self {
            adapter,
            idx: 0,
        }
    }
    fn get_display_by_idx(adapter: &Adapter, idx: u32) -> Option<Display> {
        let output = unsafe { adapter.0.EnumOutputs(idx) };
        match output {
            Ok(output) => Some(Display::new(output.cast().unwrap())),
            Err(_) => None
        }
    }
}

impl Iterator for DisplayIterator {
    type Item = Display;

    fn next(&mut self) -> Option<Self::Item> {
        let out = Self::get_display_by_idx(&self.adapter, self.idx);
        if out.is_some() {
            self.idx += 1;
        } else {
            self.idx = 0;
        }
        out
    }
}


pub struct AdapterFactory {
    fac: IDXGIFactory6,
    count: u32,
}

unsafe impl Send for AdapterFactory {}

unsafe impl Sync for AdapterFactory {}

impl Default for AdapterFactory {
    fn default() -> Self {
        AdapterFactory::new()
    }
}

impl AdapterFactory {

    pub fn new() -> Self {
        unsafe {
            let dxgi_factory: IDXGIFactory6 = CreateDXGIFactory2(0).unwrap();
            Self {
                fac: dxgi_factory,
                count: 0,
            }
        }
    }

    pub fn get_adapter_by_idx(&self, idx: u32) -> Option<Adapter> {
        let adapter = unsafe { self.fac.EnumAdapterByGpuPreference(idx, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE) };
        adapter.ok().map(Adapter)
    }

    pub fn get_adapter_by_luid(&self, luid: LUID) -> Option<Adapter> {
        let adapter = unsafe { self.fac.EnumAdapterByLuid(luid) };
        adapter.ok().map(Adapter)
    }

    pub fn reset(&mut self) {
        self.count = 0;
    }

    pub fn as_raw_ref(&self) -> &IDXGIFactory6 {
        &self.fac
    }
}

impl Iterator for AdapterFactory {
    type Item = Adapter;

    fn next(&mut self) -> Option<Self::Item> {
        let adapter = self.get_adapter_by_idx(self.count);
        self.count += 1;
        if adapter.is_none() {
            self.count = 0;
        }
        adapter
    }
}
