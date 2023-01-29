#![windows_subsystem = "windows"]

mod cursor_tracker;
mod vsync_helper;
mod utils;
mod directx;
mod config;

use std::ops::Add;
use std::ptr::null;
use std::time::{Duration, Instant};
use anyhow::Context;
use glam::{Mat4, Quat, vec3};
use log::LevelFilter;
use windows::Win32::Graphics::Gdi::HMONITOR;
use tao::{event::*, event_loop::*, window::*};
use tao::menu::{MenuItemAttributes, MenuType};
use tao::menu::ContextMenu;
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::platform::windows::{IconExtWindows, WindowBuilderExtWindows};
use tao::system_tray::SystemTrayBuilder;
use windows::Win32::Graphics::Direct3D11::{D3D11_BLEND_INV_DEST_COLOR, D3D11_BLEND_INV_SRC_ALPHA, D3D11_BLEND_INV_SRC_COLOR, D3D11_BLEND_ONE, D3D11_BLEND_SRC_ALPHA, D3D11_BLEND_SRC_COLOR, D3D11_BLEND_ZERO, D3D11_VIEWPORT};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use crate::config::Config;
use crate::directx::{AdapterFactory, CursorSprite, CursorType, DesktopDuplication, Direct3D, QuadRenderer};
use crate::utils::{LogResultExt, make_blend_state, make_shader_resource_view, show_message_box};

#[derive(Debug, Clone, Copy)]
pub enum CustomEvent {
    CursorMonitorSwitch(HMONITOR),
    VBlank,
    ConfigChange
}

fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        //.filter(Some("desktop_display::directx::duplication"), LevelFilter::Debug)
        .format_timestamp(None)
        //.format_target(false)
        .init();

    match run() {
        Ok(()) => Ok(()),
        Err(err) => {
            show_message_box("Error", format!("{}", err));
            Err(err)
        }
    }
}

fn run() -> anyhow::Result<()> {


    unsafe { SetProcessDpiAwarenessContext(Some(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)); }
    unsafe { CoInitializeEx(Some(null()), COINIT_MULTITHREADED)?; }

    let mut config = Config::load()?;

    let adapter = AdapterFactory::new()?
        .get_adapter_by_idx(0)
        .context("Can not get default graphics adapter")?;

    let mut event_loop = EventLoop::with_user_event();
    let window = WindowBuilder::new()
        .with_visible(false)
        .with_title("Display Peek")
        .with_drag_and_drop(false)
        .with_decorations(false)
        .with_always_on_top(true)
        .with_skip_taskbar(true)
        .with_undecorated_shadow(true)
        .build(&event_loop)?;
    window.set_ignore_cursor_events(true)?;
    let _tracker = cursor_tracker::set_hook(&event_loop)?;
    let vsync_switcher = vsync_helper::start_vsync_thread(&event_loop, None);
    let _config_watcher = Config::create_watcher(&event_loop)?;


    let mut tray_menu = ContextMenu::new();
    let _version_item = tray_menu.add_item(MenuItemAttributes::new(concat!("Display Peek (version ", env!("CARGO_PKG_VERSION"), ")"))
        .with_enabled(false));
    let config_item = tray_menu.add_item(MenuItemAttributes::new("Open Config"));
    let quit_item = tray_menu.add_item(MenuItemAttributes::new("Quit"));
    let _tray = SystemTrayBuilder::new(Icon::from_resource(32512, None)?, Some(tray_menu))
        .with_tooltip("Display Peek")
        .build(&event_loop)?;

    let mut d3d = Direct3D::new(&adapter, &window)?;
    let quad_renderer = QuadRenderer::new(&d3d)?;

    let mut dupl: Option<DesktopDuplication> = None;
    let mut cursor_sprite = CursorSprite::new(&d3d.device, 32, 32)?;

    let blend_state_color = make_blend_state(&d3d.device, D3D11_BLEND_ONE, D3D11_BLEND_INV_SRC_ALPHA)?;

    let blend_state_monochrome_1 = make_blend_state(&d3d.device, D3D11_BLEND_ZERO, D3D11_BLEND_SRC_COLOR)?;
    let blend_state_monochrome_2 = make_blend_state(&d3d.device, D3D11_BLEND_INV_DEST_COLOR, D3D11_BLEND_INV_SRC_COLOR)?;

    let blend_state_masked_1 = make_blend_state(&d3d.device, D3D11_BLEND_SRC_ALPHA, D3D11_BLEND_INV_SRC_ALPHA)?;
    let blend_state_masked_2 = make_blend_state(&d3d.device, D3D11_BLEND_INV_DEST_COLOR, D3D11_BLEND_INV_SRC_COLOR)?;

    let reload_state = {
        let proxy = event_loop.create_proxy();
        move || match cursor_tracker::get_current_monitor() {
            None => log::warn!("Can not get current monitor"),
            Some(monitor) => proxy.send_event(CustomEvent::CursorMonitorSwitch(monitor))
                .unwrap_or_else(|_|log::warn!("Can not send reload event to eventloop"))
        }
    };
    reload_state();

    let mut reload_timer: Option<Instant> = None;

    event_loop.run_return(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::RedrawRequested(_) => {
                if let Some(dupl) = dupl.as_ref() {
                    if let Some(tex) = dupl
                        .get_frame()
                        .and_then(|tex|
                            make_shader_resource_view(&d3d.device, tex)
                                .log_ok("Can not make shader resource view")) {
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
                            quad_renderer.draw(&d3d, transform, &tex);

                            if let (Some(pt), true) = (dupl.get_cursor_pos(), cursor_sprite.valid) {
                                let transform = screenspace * Mat4::from_scale_rotation_translation(
                                    vec3(cursor_sprite.width as f32, cursor_sprite.height as f32, 0.0),
                                    Quat::IDENTITY,
                                    vec3(pt.x as f32, pt.y as f32, 0.0)
                                );
                                match cursor_sprite.cursor_type {
                                    CursorType::Color => {
                                        d3d.context.OMSetBlendState(&blend_state_color, None, u32::MAX);
                                        quad_renderer.draw(&d3d, transform, cursor_sprite.norm_srv());
                                    }
                                    CursorType::Monochrome => {
                                        d3d.context.OMSetBlendState(&blend_state_monochrome_1, None, u32::MAX);
                                        quad_renderer.draw(&d3d, transform, cursor_sprite.norm_srv());
                                        d3d.context.OMSetBlendState(&blend_state_monochrome_2, None, u32::MAX);
                                        quad_renderer.draw(&d3d, transform, cursor_sprite.mask_srv());
                                    }
                                    CursorType::MaskedColor => {
                                        d3d.context.OMSetBlendState(&blend_state_masked_1, None, u32::MAX);
                                        quad_renderer.draw(&d3d, transform, cursor_sprite.norm_srv());
                                        d3d.context.OMSetBlendState(&blend_state_masked_2, None, u32::MAX);
                                        quad_renderer.draw(&d3d, transform, cursor_sprite.mask_srv());
                                    }
                                }

                            }
                            //TODO only swap dirty rects
                            d3d.swap_chain.Present(1, 0).ok()
                                .expect("Swap chain error");
                        }
                    }
                }
            },
            Event::UserEvent(CustomEvent::CursorMonitorSwitch(monitor)) => {
                match adapter.get_display_by_handle(monitor) {
                    None => log::warn!("Cannot find the correct display"),
                    Some(display) => match display
                        .name()
                        .log_ok("Can not get monitor name")
                        .and_then(|n|config.get_overlay_config(&n)) {
                        None => {
                            dupl = None;
                            vsync_switcher.change_display(None);
                            window.set_visible(false);
                        }
                        Some(overlay_config) => {
                            let equals = dupl.as_ref().map(|d| d.get_current_output() == &display);
                            if !equals.unwrap_or(false) {
                                dupl.take();
                                match DesktopDuplication::new(&d3d.device, display) {
                                    Ok(new_dupl) => {
                                        vsync_switcher.change_display(new_dupl.get_current_output().clone());
                                        dupl = Some(new_dupl);
                                    }
                                    Err(err) => log::error!("Can not create desktop duplication: {}", err)
                                };
                            }
                            window.set_outer_position(overlay_config.position);
                            window.set_inner_size(overlay_config.size);
                            window.set_visible(true);
                        }
                    }
                }
            },
            Event::UserEvent(CustomEvent::VBlank) => {
                if let Some(dupl) = dupl.as_mut() {
                    match dupl.try_acquire_next_frame() {
                        Ok(result) => {
                            if result.success {
                                window.request_redraw()
                            }
                            if result.cursor_updated {
                                let cursor_data = dupl.get_cursor_data().expect("The cursor should be available");
                                cursor_sprite.update(&d3d.device, &d3d.context, cursor_data)
                                    .log_ok("Can not update cursor");
                            }
                        },
                        Err(err) => log::error!("error acquiring frame: {}", err)
                    }
                }
            },
            Event::UserEvent(CustomEvent::ConfigChange) => {
                log::trace!("Config modified");
                let timer = reload_timer.insert(Instant::now().add(Duration::from_secs_f32(0.25)));
                *control_flow = ControlFlow::WaitUntil(*timer);
            },
            Event::NewEvents(_) => {
                if let Some(timer) = reload_timer {
                    if timer.checked_duration_since(Instant::now()).is_none() {
                        log::debug!("Reloading config");
                        reload_timer = None;
                        match Config::load() {
                            Ok(new_config) => {
                                config = new_config;
                                reload_state();
                            },
                            Err(err) => {
                                log::error!("Error loading config: {}", err);
                            }
                        }
                    }
                }
            }
            Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
               d3d.resize(size.width, size.height)
                   .log_ok("Can not resize resources");
            }
            Event::MenuEvent { menu_id, origin: MenuType::ContextMenu, .. } => {
                if menu_id == quit_item.clone().id() {
                    *control_flow = ControlFlow::Exit;
                }
                if menu_id == config_item.clone().id() {
                    if let Err(err) = open::that(Config::path()) {
                        log::warn!("Can not open editor: {}", err);
                        show_message_box("Error", format!("Can not open editor\n{}", err));
                    }
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
