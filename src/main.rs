#![windows_subsystem = "windows"]

mod cursor_tracker;
mod vsync_helper;
mod utils;
mod directx;

use std::mem::size_of;
use std::ptr::null;
use log::LevelFilter;
use mltg::{CompositeMode, Interpolation};
use windows::Win32::Graphics::Gdi::HMONITOR;
use tao::{dpi::*, event::*, event_loop::*, window::*};
use tao::keyboard::Key;
use tao::menu::{MenuItemAttributes, MenuType};
use tao::menu::ContextMenu;
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::platform::windows::{IconExtWindows, WindowBuilderExtWindows};
use tao::system_tray::SystemTrayBuilder;
use windows::core::Interface;
use windows::s;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::{D3D11_APPEND_ALIGNED_ELEMENT, D3D11_BIND_INDEX_BUFFER, D3D11_BIND_VERTEX_BUFFER, D3D11_BUFFER_DESC, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_DEBUG, D3D11_CREATE_DEVICE_FLAG, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_INPUT_ELEMENT_DESC, D3D11_INPUT_PER_VERTEX_DATA, D3D11_SAMPLER_DESC, D3D11_SDK_VERSION, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DEFAULT, D3D11_VIEWPORT, D3D11CreateDevice, ID3D11DeviceContext4, ID3D11RenderTargetView, ID3D11Texture2D};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_1, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_SCALING_NONE, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIFactory2};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R32_UINT, DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R32G32B32_FLOAT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use crate::directx::{AdapterFactory, CursorType, DesktopDuplicationApi, Direct3D, QuadRenderer};

#[derive(Debug, Clone, Copy)]
pub enum CustomEvent {
    CursorMonitorSwitch(HMONITOR),
    VBlank
}

fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        .filter(Some("desktop_display::directx::duplication"), LevelFilter::Debug)
        .format_timestamp(None)
        //.format_target(false)
        .init();

    unsafe { SetProcessDpiAwarenessContext(Some(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)); }
    unsafe { CoInitializeEx(Some(null()), COINIT_MULTITHREADED)?; }

    let adapter = AdapterFactory::new().get_adapter_by_idx(0).unwrap();
    let mut display_iter = adapter.iter_displays().cycle();
    let output = display_iter.next().unwrap();

    let mut event_loop = EventLoop::with_user_event();
    let window = WindowBuilder::new()
        .with_title("mltg d2d")
        .with_drag_and_drop(false)
        .with_inner_size(LogicalSize::new(1280, 720))
        //.with_decorations(false)
        //.with_always_on_top(true)
        //.with_skip_taskbar(true)
        .build(&event_loop)?;
    let _tracker = cursor_tracker::set_hook(&event_loop);
    let vsync_switcher = vsync_helper::start_vsync_thread(&event_loop, output.clone());

    let mut tray_menu = ContextMenu::new();
    let quit_item = tray_menu.add_item(MenuItemAttributes::new("Quit"));
    let _tray = SystemTrayBuilder::new(Icon::from_resource(32512, None)?, Some(tray_menu))
        .with_tooltip("Window Peek")
        .build(&event_loop)?;

    /*
    let ctx = mltg::Context::new(mltg::Direct2D::new(adapter.as_raw_ref())?)?;
    let factory = ctx.create_factory();
 */

    let mut d3d = Direct3D::new(&adapter, &window)?;
    let quad_renderer = QuadRenderer::new(&d3d)?;

    let mut dupl = DesktopDuplicationApi::new_with(
        d3d.device.clone(),
        d3d.context.clone(),
        output).unwrap();


    let sampler = unsafe {
        let mut sampler = std::mem::zeroed();
        d3d.device.CreateSamplerState(&D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MinLOD: f32::MIN,
            MaxLOD: f32::MAX,
            MaxAnisotropy: 1,
            MipLODBias: 0.0,
            ..Default::default()
        }, Some(&mut sampler))?;
        sampler.unwrap()
    };

    /*
    let mut render_target = ctx.create_render_target(
        window.raw_window_handle(),
        (window_size.width, window_size.height),
    )?;
     */
    let mut fps = fps_counter::FPSCounter::new();

    event_loop.run_return(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::RedrawRequested(_) => {
                if let Some(tex) = dupl.get_frame() {
                    unsafe {
                        let window_size = window.inner_size();


                        d3d.context.ClearRenderTargetView(d3d.render_target(), [0.0, 1.0, 1.0, 1.0].as_ptr());


                        d3d.context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                            Width: window_size.width as f32,
                            Height: window_size.height as f32,
                            MaxDepth: 1.0,
                            ..Default::default()
                        }]));
                        d3d.context.OMSetRenderTargets(Some(&[d3d.render_target().clone()]), None);
                        //d3d.context.OMSetBlendState(&blend, None, u32::MAX);

                        quad_renderer.bind(&d3d);
                        let tex_view = {
                            let mut view = std::mem::zeroed();
                            d3d.device.CreateShaderResourceView(tex, None, Some(&mut view)).unwrap();
                            view.unwrap()
                        };
                        quad_renderer.draw(&d3d, &sampler, &tex_view);

                        d3d.swap_chain.Present(1, 0).unwrap();
                    }
                }
                /*
                if let Some(tex) = dupl.get_frame() {
                    let window_size = window.inner_size().to_logical::<f32>(window.scale_factor());
                    ctx.set_scale_factor(window.scale_factor() as _);
                    ctx.draw(&render_target, |cmd| {
                        cmd.clear((0.0, 0.0, 0.3, 0.0));
                        let mut desc = Default::default();
                        unsafe { tex.GetDesc(&mut desc); }
                        let aspect = desc.Width as f32 / desc.Height as  f32;
                        let width = window_size.width.min(window_size.height * aspect);
                        let height = width / aspect;
                        let x = 0.5 * (window_size.width - width);
                        let y = 0.5 * (window_size.height - height);
                        let scale = height / desc.Height as f32;
                        let interpolation = if scale > 0.6 {
                            Interpolation::Cubic
                        } else {
                            Interpolation::HighQualityCubic
                        };
                        cmd.set_transform(x, y, scale);
                        cmd.draw_bitmap2(&factory.create_bitmap(tex).unwrap(), (0.0,0.0), interpolation, CompositeMode::SourceOver);

                        if let Some((cursor, cursor_type)) = dupl.get_cursor() {
                            match cursor_type {
                                CursorType::Color(bitmap) => {
                                    cmd.draw_bitmap2(bitmap, (cursor.x as f32, cursor.y as f32), interpolation, CompositeMode::SourceOver);
                                }
                                CursorType::Monochrome(and, xor) => {
                                    cmd.draw_bitmap2(and, (cursor.x as f32, cursor.y as f32), interpolation, CompositeMode::SourceOver);
                                    cmd.draw_bitmap2(xor, (cursor.x as f32, cursor.y as f32), interpolation, CompositeMode::MaskInvert);
                                },
                                CursorType::MaskedColor(and, xor) => {
                                    cmd.draw_bitmap2(and, (cursor.x as f32, cursor.y as f32), interpolation, CompositeMode::SourceOver);
                                    cmd.draw_bitmap2(xor, (cursor.x as f32, cursor.y as f32), interpolation, CompositeMode::MaskInvert);
                                }
                            }
                        }
                    }).unwrap();
                }
                window.set_title(&format!("{} fps", fps.tick()))
                 */
            },
            Event::UserEvent(CustomEvent::CursorMonitorSwitch(monitor)) => {
                //let monitor: winit::monitor::MonitorHandle = unsafe {std::mem::transmute(monitor)};

                match adapter.iter_displays().find(|d|d.hmonitor().unwrap() == monitor) {
                    None => log::warn!("Cannot find the correct display"),
                    Some(display) => {
                        dupl.switch_output(display.clone()).unwrap();
                        vsync_switcher.change_display(display);
                    }
                }

                //log::info!("Cursor event: {:?}", monitor);
            },
            Event::UserEvent(CustomEvent::VBlank) => {
                if let Ok(()) = dupl.try_acquire_next_frame() {
                    window.request_redraw();
                }
            },
            Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
                //ctx.resize_target(&mut render_target, (size.width, size.height))
                //    .unwrap();
               d3d.resize(size.width, size.height).unwrap();

            }
            Event::MenuEvent { menu_id, origin: MenuType::ContextMenu, .. } => {
                if menu_id == quit_item.clone().id() {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent { event: WindowEvent::KeyboardInput { event: KeyEvent {
                logical_key: Key::F1,
                state: ElementState::Pressed,
                .. }, .. }, .. } => {
                //let display = display_iter.next().unwrap();
                //dupl.switch_output(display.clone()).unwrap();
                //vsync_switcher.change_display(display.clone());
                //log::info!("Current Display: {:?}", display.name());
            }
            Event::WindowEvent { event: WindowEvent::KeyboardInput { event: KeyEvent {
               logical_key: Key::Escape,
               state: ElementState::Pressed,
                .. }, .. }, .. } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
    Ok(())
}
