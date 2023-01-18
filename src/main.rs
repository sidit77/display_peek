use log::LevelFilter;
use raw_window_handle::HasRawWindowHandle;
use win_desktop_duplication::{co_init, DesktopDuplicationApi, set_process_dpi_awareness};
use win_desktop_duplication::devices::AdapterFactory;
use winit::{dpi::*, event::*, event_loop::*, window::*};
use winit::platform::windows::WindowBuilderExtWindows;

fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        .filter(Some("win_desktop_duplication::duplication"), LevelFilter::Warn)
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
    let output = adapter.get_display_by_idx(0).unwrap();

    let mut dupl = DesktopDuplicationApi::new(adapter, output.clone()).unwrap();
    let (device, _) = dupl.get_device_and_ctx();

    let ctx = mltg::Context::new(mltg::Direct2D::from_d3d(&device)?)?;
    let factory = ctx.create_factory();
    let window_size = window.inner_size();
    let mut render_target = ctx.create_render_target(
        window.raw_window_handle(),
        (window_size.width, window_size.height),
    )?;

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
                        cmd.draw_bitmap(&factory.create_bitmap(tex.as_raw_ref()).unwrap(),
                                        mltg::Rect::from_points((10.0, 10.0), (window_size.width - 10.0, window_size.height - 10.0)),
                                        None,
                                        mltg::Interpolation::HighQualityCubic);
                    }).unwrap();
                }

            },
            Event::MainEventsCleared => {
                output.wait_for_vsync().unwrap();
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
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}
