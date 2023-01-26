use windows::Win32::Graphics::Direct3D11::{D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext4, ID3D11RenderTargetView, ID3D11Texture2D};
use anyhow::Result;
use tao::platform::windows::WindowExtWindows;
use tao::window::Window;
use windows::core::Interface;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_1};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_SCALING_NONE, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIFactory2, IDXGISwapChain1};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};
use crate::directx::Adapter;

pub struct Direct3D {
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext4,
    pub swap_chain: IDXGISwapChain1,
    render_target: Option<ID3D11RenderTargetView>
}

impl Direct3D {

    pub fn new(adapter: &Adapter, window: &Window) -> Result<Self> {
        let mut d3d_device = None;
        let mut d3d_ctx = None;
        unsafe {
            D3D11CreateDevice(
                adapter.as_raw_ref(),
                D3D_DRIVER_TYPE_UNKNOWN,
                None,
                D3D11_CREATE_DEVICE_FLAG(0),
                Some(&[D3D_FEATURE_LEVEL_11_1]),
                D3D11_SDK_VERSION,
                Some(&mut d3d_device),
                None,
                Some(&mut d3d_ctx),
            )?;
        }
        let d3d_device = d3d_device.unwrap();
        let d3d_ctx = d3d_ctx.unwrap().cast::<ID3D11DeviceContext4>()?;

        let dxgi_factory = unsafe { CreateDXGIFactory1::<IDXGIFactory2>()? };
        let window_size = window.inner_size();
        let swap_chain = unsafe {
            dxgi_factory.CreateSwapChainForHwnd(
                &d3d_device,
                HWND(window.hwnd() as _),
                &DXGI_SWAP_CHAIN_DESC1 {
                    Width: window_size.width,
                    Height: window_size.height,
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    BufferCount: 2,
                    BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                    SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                    Scaling: DXGI_SCALING_NONE,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    ..Default::default()
                },
                None,
                None,
            )?
        };

        let mut rtv = unsafe {
            let buffer = swap_chain.GetBuffer::<ID3D11Texture2D>(0)?;
            let mut target = std::mem::zeroed();
            d3d_device.CreateRenderTargetView(&buffer, None, Some(&mut target))?;
            target.unwrap()
        };

        Ok(Self {
            device: d3d_device,
            context: d3d_ctx,
            swap_chain,
            render_target: Some(rtv),
        })
    }

    pub fn render_target(&self) -> &ID3D11RenderTargetView {
        self.render_target.as_ref().unwrap()
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        unsafe {
            self.context.OMSetRenderTargets(None, None);
            self.render_target = None;
            self.swap_chain.ResizeBuffers(0, width, height, DXGI_FORMAT_UNKNOWN, 0)?;
            let buffer = self.swap_chain.GetBuffer::<ID3D11Texture2D>(0).unwrap();
            self.device.CreateRenderTargetView(&buffer, None, Some(&mut self.render_target))?;
            Ok(())
        }
    }

}