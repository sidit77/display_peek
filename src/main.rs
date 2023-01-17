use raw_window_handle::HasRawWindowHandle;
use winit::{dpi::*, event::*, event_loop::*, window::*};

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("mltg d2d")
        .with_inner_size(LogicalSize::new(1280, 720))
        .build(&event_loop)?;
    let ctx = mltg::Context::new(mltg::Direct2D::new()?)?;
    let factory = ctx.create_factory();
    let window_size = window.inner_size();
    let mut render_target = ctx.create_render_target(
        window.raw_window_handle(),
        (window_size.width, window_size.height),
    )?;
    let brush = factory.create_solid_color_brush((1.0, 1.0, 1.0, 1.0))?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::RedrawRequested(_) => {
                let window_size = window.inner_size().to_logical::<f32>(window.scale_factor());
                ctx.set_scale_factor(window.scale_factor() as _);
                ctx.draw(&render_target, |cmd| {
                    cmd.clear((0.0, 0.0, 0.3, 0.0));
                    cmd.fill(
                        &mltg::Rect::from_points((10.0, 10.0), (window_size.width - 10.0, window_size.height - 10.0)),
                        &brush,
                    );
                })
                    .unwrap();
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
