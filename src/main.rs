#![windows_subsystem = "windows"]

mod cursor_tracker;
mod vsync_helper;

use log::LevelFilter;
use mltg::{CompositeMode, Interpolation};
use raw_window_handle::HasRawWindowHandle;
use win_desktop_duplication::{co_init, CursorType, DesktopDuplicationApi, set_process_dpi_awareness};
use win_desktop_duplication::devices::AdapterFactory;
use windows::Win32::Graphics::Gdi::HMONITOR;
use winit::{dpi::*, event::*, event_loop::*, window::*};
use winit::platform::run_return::EventLoopExtRunReturn;
use winit::platform::windows::WindowBuilderExtWindows;

#[derive(Debug, Clone, Copy)]
pub enum CustomEvent {
    CursorMonitorSwitch(HMONITOR),
    VBlank
}


fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        .filter(Some("win_desktop_duplication::duplication"), LevelFilter::Debug)
        .format_timestamp(None)
        //.format_target(false)
        .init();

    set_process_dpi_awareness();
    co_init();

    let adapter = AdapterFactory::new().get_adapter_by_idx(0).unwrap();
    let mut display_iter = adapter.iter_displays().cycle();
    let output = display_iter.next().unwrap();

    let mut event_loop = EventLoopBuilder::with_user_event().build();
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

    let ctx = mltg::Context::new(mltg::Direct2D::new(adapter.as_raw_ref())?)?;
    let factory = ctx.create_factory();

    let mut dupl = DesktopDuplicationApi::new_with(
        ctx.backend.d3d11_device.clone(),
        ctx.backend.d3d11_ctx.clone(),
        ctx.d2d1_device_context.clone(),
        output.clone()).unwrap();

    let window_size = window.inner_size();
    let mut render_target = ctx.create_render_target(
        window.raw_window_handle(),
        (window_size.width, window_size.height),
    )?;

    let mut fps = fps_counter::FPSCounter::new();

    event_loop.run_return(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::RedrawRequested(_) => {
                if let Some(tex) = dupl.get_frame() {
                    let window_size = window.inner_size().to_logical::<f32>(window.scale_factor());
                    ctx.set_scale_factor(window.scale_factor() as _);
                    ctx.draw(&render_target, |cmd| {
                        cmd.clear((0.0, 0.0, 0.3, 0.0));
                        let aspect = tex.desc().width as f32 / tex.desc().height as  f32;
                        let width = window_size.width.min(window_size.height * aspect);
                        let height = width / aspect;
                        let x = 0.5 * (window_size.width - width);
                        let y = 0.5 * (window_size.height - height);
                        let scale = height / tex.desc().height as f32;
                        let interpolation = if scale > 0.6 {
                            Interpolation::Cubic
                        } else {
                            Interpolation::HighQualityCubic
                        };
                        cmd.set_transform(x, y, scale);
                        cmd.draw_bitmap2(&factory.create_bitmap(tex.as_raw_ref()).unwrap(), (0.0,0.0), interpolation, CompositeMode::SourceOver);

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
            },
            Event::UserEvent(CustomEvent::CursorMonitorSwitch(monitor)) => {
                //let monitor: winit::monitor::MonitorHandle = unsafe {std::mem::transmute(monitor)};
                match adapter.iter_displays().filter(|d|d.hmonitor() == monitor).next() {
                    None => log::warn!("Cannot find the correct display"),
                    Some(display) => {
                        dupl.switch_output(display.clone()).unwrap();
                        vsync_switcher.change_display(display);
                    }
                }
            },
            Event::UserEvent(CustomEvent::VBlank) => {
                if let Ok(()) = dupl.try_acquire_next_frame() {
                    window.request_redraw();
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                ctx.resize_target(&mut render_target, (size.width, size.height))
                    .unwrap();
            }
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { input: KeyboardInput{
                    state: ElementState::Pressed,
                    virtual_keycode: Some(VirtualKeyCode::F1),
                    ..
                }, .. },
                ..
            } => {
                let display = display_iter.next().unwrap();
                dupl.switch_output(display.clone()).unwrap();
                vsync_switcher.change_display(display.clone());
                log::info!("Current Display: {:?}", display.name());
            }
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { input: KeyboardInput{
                    state: ElementState::Pressed,
                    virtual_keycode: Some(VirtualKeyCode::Escape),
                    ..
                }, .. },
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
    Ok(())
}
