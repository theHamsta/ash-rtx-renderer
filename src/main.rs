use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::vulkan_app::VulkanApp;

mod vulkan_app;

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_position(winit::dpi::PhysicalPosition::new(1300, 800))
        .build(&event_loop)
        .unwrap();
    let _vulkan_app = VulkanApp::new(&window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        let mut exit = || *control_flow = ControlFlow::Exit;

        match event {
            Event::WindowEvent { event, window_id } if window_id == window.id() => match event {
                WindowEvent::CloseRequested => exit(),
                WindowEvent::KeyboardInput { input, .. } => match input.virtual_keycode {
                    Some(winit::event::VirtualKeyCode::Escape) => exit(),

                    _ => (),
                },
                _ => (),
            },
            _ => (),
        }
    });
}
