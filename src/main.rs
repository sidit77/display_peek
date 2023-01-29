#![windows_subsystem = "windows"]

mod cursor_tracker;
mod vsync_helper;
mod utils;
mod directx;
mod config;

use std::ptr::null;
use glam::{Mat4, Quat, vec3};
use log::LevelFilter;
use windows::Win32::Graphics::Gdi::HMONITOR;
use tao::{dpi::*, event::*, event_loop::*, window::*};
use tao::menu::{MenuItemAttributes, MenuType};
use tao::menu::ContextMenu;
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::platform::windows::{IconExtWindows, WindowBuilderExtWindows};
use tao::system_tray::SystemTrayBuilder;
use windows::Win32::Graphics::Direct3D11::{D3D11_BLEND_INV_DEST_COLOR, D3D11_BLEND_INV_SRC_ALPHA, D3D11_BLEND_INV_SRC_COLOR, D3D11_BLEND_ONE, D3D11_BLEND_SRC_ALPHA, D3D11_BLEND_SRC_COLOR, D3D11_BLEND_ZERO, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_SAMPLER_DESC, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_VIEWPORT};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use crate::config::Config;
use crate::directx::{AdapterFactory, CursorType, DesktopDuplicationApi, Direct3D, QuadRenderer};
use crate::utils::make_blend_state;

#[derive(Debug, Clone, Copy)]
pub enum CustomEvent {
    CursorMonitorSwitch(HMONITOR),
    VBlank
}

fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        //.filter(Some("desktop_display::directx::duplication"), LevelFilter::Debug)
        .format_timestamp(None)
        //.format_target(false)
        .init();

    unsafe { SetProcessDpiAwarenessContext(Some(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)); }
    unsafe { CoInitializeEx(Some(null()), COINIT_MULTITHREADED)?; }

    let mut config = Config::load();

    let adapter = AdapterFactory::new().get_adapter_by_idx(0).unwrap();

    let mut event_loop = EventLoop::with_user_event();
    let window = WindowBuilder::new()
        .with_visible(false)
        .with_title("DisplayPeek")
        .with_drag_and_drop(false)
        .with_inner_size(LogicalSize::new(1280, 720))
        .with_position(LogicalPosition::new(-1600, 150))
        .with_decorations(false)
        .with_always_on_top(true)
        .with_skip_taskbar(true)
        .with_undecorated_shadow(true)
        .build(&event_loop)?;
    window.set_ignore_cursor_events(true)?;
    let _tracker = cursor_tracker::set_hook(&event_loop);
    let vsync_switcher = vsync_helper::start_vsync_thread(&event_loop, None);

    let mut tray_menu = ContextMenu::new();
    let config_item = tray_menu.add_item(MenuItemAttributes::new("Open Config"));
    let quit_item = tray_menu.add_item(MenuItemAttributes::new("Quit"));
    let _tray = SystemTrayBuilder::new(Icon::from_resource(32512, None)?, Some(tray_menu))
        .with_tooltip("DisplayPeek")
        .build(&event_loop)?;

    let mut d3d = Direct3D::new(&adapter, &window)?;
    let quad_renderer = QuadRenderer::new(&d3d)?;

    let mut dupl: Option<DesktopDuplicationApi> = None;

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

    let blend_state_color = make_blend_state(&d3d.device, D3D11_BLEND_ONE, D3D11_BLEND_INV_SRC_ALPHA)?;

    let blend_state_monochrome_1 = make_blend_state(&d3d.device, D3D11_BLEND_ZERO, D3D11_BLEND_SRC_COLOR)?;
    let blend_state_monochrome_2 = make_blend_state(&d3d.device, D3D11_BLEND_INV_DEST_COLOR, D3D11_BLEND_INV_SRC_COLOR)?;

    let blend_state_masked_1 = make_blend_state(&d3d.device, D3D11_BLEND_SRC_ALPHA, D3D11_BLEND_INV_SRC_ALPHA)?;
    let blend_state_masked_2 = make_blend_state(&d3d.device, D3D11_BLEND_INV_DEST_COLOR, D3D11_BLEND_INV_SRC_COLOR)?;

    let reload_state = {
        let proxy = event_loop.create_proxy();
        move || proxy.send_event(CustomEvent::CursorMonitorSwitch(cursor_tracker::get_current_monitor())).unwrap_or_default()
    };
    reload_state();

    event_loop.run_return(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::RedrawRequested(_) => {
                if let Some(dupl) = dupl.as_ref() {
                    if let Some(tex) = dupl.get_frame() {
                        unsafe {
                            let window_size = window.inner_size();
                            let screenspace = Mat4::orthographic_rh(
                                0.0,
                                window_size.width as f32,
                                window_size.height as f32,
                                0.0,
                                -1.0,
                                1.0);

                            d3d.context.ClearRenderTargetView(d3d.render_target(), [0.0, 0.0, 0.3, 1.0].as_ptr());

                            d3d.context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                                Width: window_size.width as f32,
                                Height: window_size.height as f32,
                                MaxDepth: 1.0,
                                ..Default::default()
                            }]));
                            d3d.context.OMSetRenderTargets(Some(&[d3d.render_target().clone()]), None);

                            quad_renderer.bind(&d3d);
                            let tex_view = {
                                let mut view = std::mem::zeroed();
                                d3d.device.CreateShaderResourceView(tex, None, Some(&mut view)).unwrap();
                                view.unwrap()
                            };

                            let (display_width, display_height) = dupl.get_display_mode().get_flipped_size();
                            let aspect = display_width as f32 / display_height as  f32;
                            let width = (window_size.width as f32).min(window_size.height as f32 * aspect);
                            let height = width / aspect;
                            let x = 0.5 * (window_size.width as f32 - width);
                            let y = 0.5 * (window_size.height as f32 - height);
                            let scale = height / display_height as f32;

                            let screenspace = screenspace * Mat4::from_scale_rotation_translation(
                                vec3(scale, scale, 0.0),
                                Quat::IDENTITY,
                                vec3(x, y, 0.0)
                            );

                            let transform = screenspace * dupl.get_display_mode().get_frame_transform();
                            d3d.context.OMSetBlendState(None, None, u32::MAX);
                            quad_renderer.draw(&d3d, transform, &sampler, &tex_view);

                            if let Some((pt, cursor)) = dupl.get_cursor() {
                                let transform = screenspace * Mat4::from_scale_rotation_translation(
                                    vec3(cursor.width as f32, cursor.height as f32, 0.0),
                                    Quat::IDENTITY,
                                    vec3(pt.x as f32, pt.y as f32, 0.0)
                                );
                                match cursor.cursor_type {
                                    CursorType::Color => {
                                        d3d.context.OMSetBlendState(&blend_state_color, None, u32::MAX);
                                        quad_renderer.draw(&d3d, transform, &sampler, cursor.norm_srv());
                                    }
                                    CursorType::Monochrome => {
                                        d3d.context.OMSetBlendState(&blend_state_monochrome_1, None, u32::MAX);
                                        quad_renderer.draw(&d3d, transform, &sampler, cursor.norm_srv());
                                        d3d.context.OMSetBlendState(&blend_state_monochrome_2, None, u32::MAX);
                                        quad_renderer.draw(&d3d, transform, &sampler, cursor.mask_srv());
                                    }
                                    CursorType::MaskedColor => {
                                        d3d.context.OMSetBlendState(&blend_state_masked_1, None, u32::MAX);
                                        quad_renderer.draw(&d3d, transform, &sampler, cursor.norm_srv());
                                        d3d.context.OMSetBlendState(&blend_state_masked_2, None, u32::MAX);
                                        quad_renderer.draw(&d3d, transform, &sampler, cursor.mask_srv());
                                    }
                                }

                            }

                            d3d.swap_chain.Present(1, 0).unwrap();
                        }
                    }
                }
            },
            Event::UserEvent(CustomEvent::CursorMonitorSwitch(monitor)) => {
                match adapter.get_display_by_handle(monitor) {
                    None => log::warn!("Cannot find the correct display"),
                    Some(display) => match config.get_overlay_config(&display.name().unwrap()) {
                        None => {
                            dupl = None;
                            vsync_switcher.change_display(None);
                            window.set_visible(false);
                        }
                        Some(overlay_config) => {
                            let equals = dupl.as_ref().map(|d| d.get_current_output() == &display);
                            if !equals.unwrap_or(false) {
                                dupl.take();
                                let new_dupl = DesktopDuplicationApi::new_with(d3d.device.clone(), d3d.context.clone(), display).unwrap();//.switch_output(display).unwrap();
                                vsync_switcher.change_display(new_dupl.get_current_output().clone());
                                dupl = Some(new_dupl);
                            }
                            window.set_outer_position(overlay_config.position);
                            window.set_inner_size(overlay_config.size);
                            window.set_visible(true);
                        }
                    }
                }

                //log::info!("Cursor event: {:?}", monitor);
            },
            Event::UserEvent(CustomEvent::VBlank) => {
                if let Some(dupl) = dupl.as_mut() {
                    match dupl.try_acquire_next_frame() {
                        Ok(true) => window.request_redraw(),
                        Ok(false) => {},
                        Err(err) => log::error!("error aquiring frame: {}", err)
                    }
                }
            },
            Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
               d3d.resize(size.width, size.height).unwrap();

            }
            Event::MenuEvent { menu_id, origin: MenuType::ContextMenu, .. } => {
                if menu_id == quit_item.clone().id() {
                    *control_flow = ControlFlow::Exit;
                }
                if menu_id == config_item.clone().id() {
                    config = Config::load();
                    reload_state();
                }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
    Ok(())
}
