#![windows_subsystem = "windows"]

use log::LevelFilter;
use mltg::{Interpolation, Rect};
use raw_window_handle::HasRawWindowHandle;
use win_desktop_duplication::{co_init, DesktopDuplicationApi, set_process_dpi_awareness};
use win_desktop_duplication::devices::AdapterFactory;
use winit::{dpi::*, event::*, event_loop::*, window::*};
use winit::platform::windows::WindowBuilderExtWindows;

fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        .filter(Some("win_desktop_duplication::duplication"), LevelFilter::Debug)
        .format_timestamp(None)
        //.format_target(false)
        .init();

    set_process_dpi_awareness();
    co_init();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("mltg d2d")
        .with_drag_and_drop(false)
        .with_inner_size(LogicalSize::new(1280, 720))
        .build(&event_loop)?;
    let adapter = AdapterFactory::new().get_adapter_by_idx(0).unwrap();
    let mut display_iter = adapter.iter_displays().cycle();
    let output = display_iter.next().unwrap();

    let ctx = mltg::Context::new(mltg::Direct2D::new(adapter.as_raw_ref())?)?;
    let mut dupl = DesktopDuplicationApi::new_with(ctx.backend.d3d11_device.clone(), ctx.backend.d3d11_ctx.clone(), output.clone()).unwrap();


    let factory = ctx.create_factory();
    let window_size = window.inner_size();
    let mut render_target = ctx.create_render_target(
        window.raw_window_handle(),
        (window_size.width, window_size.height),
    )?;

    let white_brush = factory.create_solid_color_brush((1.0, 0.0, 1.0, 1.0))?;

    let mut fps = fps_counter::FPSCounter::new();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
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
                        cmd.draw_bitmap(&factory.create_bitmap(tex.as_raw_ref()).unwrap(),
                                        mltg::Rect::new((x, y), (width, height)),
                                        None,
                                        match scale {
                                            r if r > 0.6 => Interpolation::Cubic,
                                            _ => Interpolation::HighQualityCubic
                                        });
                        if let Some(cursor) = dupl.get_cursor() {
                            cmd.fill(&Rect::new((x + scale * cursor.x as f32, y + scale * cursor.y as f32), (8.0,8.0)), &white_brush);
                        }
                    }).unwrap();
                }
                window.set_title(&format!("{} fps", fps.tick()))
            },
            Event::MainEventsCleared => {
                //output.wait_for_vsync().unwrap();
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
                log::info!("Current Display: {:?}", display.name());
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
}
