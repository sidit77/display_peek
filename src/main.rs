#![windows_subsystem = "windows"]

mod cursor_tracker;
mod vsync_helper;
mod utils;
mod directx;
mod config;
mod tray_helper;

use std::ops::Add;
use std::time::{Duration, Instant};
use anyhow::Context;
use error_tools::log::LogResultExt;
use error_tools::tao::EventLoopExtRunResult;
use glam::{Mat4, Quat, vec3};
use log::LevelFilter;
use windows::Win32::Graphics::Gdi::HMONITOR;
use tao::{event::*, event_loop::*, window::*};
use tao::platform::windows::{WindowBuilderExtWindows};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
use crate::config::Config;
use crate::directx::{AdapterFactory, CursorSprite, CursorType, DesktopDuplication, Direct3D, QuadRenderer};
use crate::tray_helper::create_system_tray;
use crate::utils::{com_initialized, make_blend_state, make_resource, retrieve};

#[derive(Debug, Clone, Copy)]
pub enum CustomEvent {
    CursorMonitorSwitch(HMONITOR),
    VBlank,
    ConfigChange,
    QuitButton
}

fn main() -> anyhow::Result<()> {
    #[cfg(not(debug_assertions))]
    error_tools::gui::set_gui_panic_hook();

    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        //.filter(Some("desktop_display::directx::duplication"), LevelFilter::Debug)
        .format_timestamp(None)
        //.format_target(false)
        .init();


    loop {
        match run().expect("Unexpected Error") {
            true => log::info!("Restarting the app"),
            false => break Ok(()),
        }
    }
}

fn run() -> anyhow::Result<bool> {

    com_initialized();

    let mut config = Config::load()?;

    let mut event_loop = EventLoop::with_user_event();
    let window = WindowBuilder::new()
        .with_visible(false)
        .with_title("Display Peek")
        .with_drag_and_drop(false)
        .with_decorations(false)
        .with_always_on_top(true)
        .with_skip_taskbar(true)
        //.with_undecorated_shadow(true)
        .with_no_redirection_bitmap(true)
        .build(&event_loop)?;
    window.set_ignore_cursor_events(true)?;
    let system_tray = create_system_tray(&event_loop)?;
    let tracker = cursor_tracker::set_hook(&event_loop)?;
    let _config_watcher = Config::create_watcher(&event_loop)?;
    let vsync_switcher = vsync_helper::start_vsync_thread(&event_loop, None);

    let adapter = AdapterFactory::new()?
        .get_adapter_by_idx(0)
        .context("Can not get default graphics adapter")?;

    let mut d3d = Direct3D::new(&adapter, &window)?;
    let quad_renderer = QuadRenderer::new(&d3d)?;

    let mut dupl: Option<DesktopDuplication> = None;
    let mut cursor_sprite = CursorSprite::new(&d3d.device, 32, 32)?;
    let mut frame_cache = CachedFrame::new();

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

    let mut last_flow = ControlFlow::Wait;
    let result = event_loop.run_result(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::MainEventsCleared => if animation_start.is_some() {
                window.request_redraw()
            },
            Event::RedrawRequested(_) => {
                if let Some(dupl) = dupl.as_ref() {
                    if let Some( tex) = frame_cache.get_view() {
                        unsafe {
                            let window_size = window.inner_size();
                            let screenspace = Mat4::orthographic_rh(
                                0.0,
                                window_size.width as f32,
                                window_size.height as f32,
                                0.0,
                                -1.0,
                                1.0);

                            d3d.context.ClearRenderTargetView(d3d.render_target(), [0.0, 0.0, 0.0, 0.0].as_ptr());

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

                            let animation = match animation_start {
                                None => 1.0,
                                Some(start) => {
                                    let mut elapsed = start.elapsed().as_secs_f32() * 3.0;
                                    if elapsed >= 1.0 {
                                        elapsed = 1.0;
                                        animation_start = None;
                                    }
                                    elapsed
                                }
                            }.powf(2.0);
                            let screenspace = screenspace * Mat4::from_scale_rotation_translation(
                                vec3(scale * animation, scale * animation, 0.0),
                                Quat::IDENTITY,
                                vec3(
                                    x + 0.5 * width * (1.0 - animation),
                                     y + 0.5 * height * (1.0 - animation), 0.0)
                            );

                            let transform = screenspace * dupl.get_display_mode().get_frame_transform();
                            d3d.context.OMSetBlendState(&blend_state_color, None, u32::MAX);
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
                            d3d.swap_chain.Present(1, 0)
                                .ok()
                                .map_err(|err| {
                                    log::error!("Swapchain error: {}", err);
                                    true
                                })?;
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
                            frame_cache.invalidate();
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
                            animation_start = Some(Instant::now());
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
                                if result.frame_update {
                                    if let Some(frame) = dupl.get_frame() {
                                        frame_cache.update(&d3d.device, &d3d.context, frame);
                                    }
                                }
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
            Event::UserEvent(CustomEvent::QuitButton) => {
                *control_flow = ControlFlow::Exit;
            }
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
                log::trace!("Resized dx resources to {}/{}", size.width, size.height);
                //window.set_undecorated_shadow(true);
            }
            Event::LoopDestroyed => {
                window.set_visible(false);
            }
            _ => {}
        }
        if animation_start.is_some() && !matches!(*control_flow, ControlFlow::ExitWithCode(_)) {
            *control_flow = ControlFlow::Poll;
        }
        if *control_flow != last_flow {
            last_flow = *control_flow;
            log::trace!("switching to {:?}", last_flow);
        }
        Ok(())
    });

    drop(tracker);
    system_tray.wait_for_end();
    Ok(match result {
        Err(true) => true,
        _ => false
    })
}

struct CachedFrame {
    resource: Option<(ID3D11Texture2D, ID3D11ShaderResourceView)>,
    valid: bool
}

impl CachedFrame {

    fn new() -> Self {
        Self {
            resource: None,
            valid: false,
        }
    }

    fn get_view(&self) -> Option<&ID3D11ShaderResourceView> {
        match self.valid {
            true => self.resource.as_ref().map(|r| &r.1),
            false => None
        }
    }

    fn invalidate(&mut self) {
        self.valid = false;
    }

    fn update(&mut self, device: &ID3D11Device, context: &ID3D11DeviceContext4, frame: &ID3D11Texture2D) {
        let frame_desc = retrieve(frame, ID3D11Texture2D::GetDesc);
        self.valid = true;
        let recreate = match &self.resource {
            None => true,
            Some((cache, _)) => {
                let cache_desc = retrieve(cache, ID3D11Texture2D::GetDesc);
                frame_desc.Width != cache_desc.Width ||
                    frame_desc.Height != cache_desc.Height ||
                    frame_desc.Format != cache_desc.Format
            }
        };
        if recreate {
            log::trace!("Creating new cache texture {}x{}", frame_desc.Width, frame_desc.Height);
            let tex = make_resource(|ptr| unsafe {
                device.CreateTexture2D(&D3D11_TEXTURE2D_DESC {
                    MipLevels: 1,
                    ArraySize: 1,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
                    CPUAccessFlags: Default::default(),
                    MiscFlags: Default::default(),
                    ..frame_desc
                }, None, ptr)
            }).log_ok("Failed to create new texture");
            let srv = tex.as_ref().and_then(|tex|
                make_resource(|ptr| unsafe {
                    device.CreateShaderResourceView(tex, None, ptr)
                }).log_ok("Failed to create new shader resource view"));
            self.resource = tex.zip(srv);
        }
        if let Some((cache, _)) = &self.resource {
            unsafe {
                context.CopyResource(cache, frame);
            }
        }
    }

}
