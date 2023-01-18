#![windows_subsystem = "windows"]

use log::LevelFilter;
use mltg::Interpolation;
use raw_window_handle::HasRawWindowHandle;
use win_desktop_duplication::{co_init, DesktopDuplicationApi, set_process_dpi_awareness};
use win_desktop_duplication::devices::AdapterFactory;
use winit::{dpi::*, event::*, event_loop::*, window::*};
use winit::platform::windows::WindowBuilderExtWindows;

fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        .filter(Some("win_desktop_duplication::duplication"), LevelFilter::Trace)
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

    let mut dupl = DesktopDuplicationApi::new(adapter, output.clone()).unwrap();
    let (device, _) = dupl.get_device_and_ctx();

    let ctx = mltg::Context::new(mltg::Direct2D::from_d3d(&device)?)?;
    let factory = ctx.create_factory();
    let window_size = window.inner_size();
    let mut render_target = ctx.create_render_target(
        window.raw_window_handle(),
        (window_size.width, window_size.height),
    )?;
    let mut fps = fps_counter::FPSCounter::new();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        match event {
            Event::RedrawRequested(_) => {
                let tex = dupl.acquire_next_frame_now();

                if let Ok(tex) = tex {
                    let window_size = window.inner_size().to_logical::<f32>(window.scale_factor());
                    ctx.set_scale_factor(window.scale_factor() as _);
                    ctx.draw(&render_target, |cmd| {
                        cmd.clear((0.0, 0.0, 0.3, 0.0));
                        let aspect = tex.desc().width as f32 / tex.desc().height as  f32;
                        let width = window_size.width.min(window_size.height * aspect);
                        let height = width / aspect;
                        cmd.draw_bitmap(&factory.create_bitmap(tex.as_raw_ref()).unwrap(),
                                        mltg::Rect::new((0.5 * (window_size.width - width), 0.5 * (window_size.height - height)),
                                                        (width, width / aspect)),
                                        None,
                                        match height / tex.desc().height as f32 {
                                            r if r > 0.6 => Interpolation::Cubic,
                                            _ => Interpolation::HighQualityCubic
                                        });
                    }).unwrap();
                    window.set_title(&format!("{} fps", fps.tick()))
                }

            },
            Event::MainEventsCleared => {
                //output.wait_for_vsync().unwrap();
                window.request_redraw();
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
