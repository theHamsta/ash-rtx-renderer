use log::{error, info};
use std::path::PathBuf;

use clap::Parser;
use mesh::Mesh;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::{
    draw_impls::{DrawImpl, Drawer, TriangleDrawer},
    vulkan_app::VulkanApp,
};

mod draw_impls;
mod mesh;
mod vulkan_app;

#[derive(clap::Parser)]
#[clap(author, version, about)]
struct Args {
    /// Mesh file to render
    #[clap(short, long)]
    mesh_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    pretty_env_logger::try_init()?;

    let args = Args::parse();
    let mesh = Mesh::from_file(&args.mesh_file, crate::mesh::ReadOptions::OnlyTriangles)?;
    info!(
        "Loaded mesh with {} triangles and {} vertices. vertex_normals: {}.",
        mesh.num_triangles(),
        mesh.num_vertices(),
        mesh.has_vertex_normals()
    );

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_position(winit::dpi::PhysicalPosition::new(1300, 800))
        .build(&event_loop)
        .unwrap();
    let mut vulkan_app = VulkanApp::new(&window)?;

    let drawers = vec![DrawImpl::Triangle(TriangleDrawer::new())];
    let mut active_drawer_idx = 0;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        let mut exit = || *control_flow = ControlFlow::Exit;

        match event {
            Event::WindowEvent { event, window_id } if window_id == window.id() => match event {
                WindowEvent::CloseRequested => exit(),
                WindowEvent::Resized(size) => vulkan_app.resize(size).unwrap(),
                WindowEvent::KeyboardInput { input, .. } => match input.virtual_keycode {
                    Some(winit::event::VirtualKeyCode::Escape) => exit(),
                    Some(
                        winit::event::VirtualKeyCode::Numpad1 | winit::event::VirtualKeyCode::Key1,
                    ) => {
                        active_drawer_idx = 0;
                        info!(
                            "Switched Drawer to {active_drawer_idx}: {:?}",
                            drawers[active_drawer_idx]
                        );
                    }
                    _ => (),
                },
                _ => (),
            },
            _ => (),
        }

        if let Err(err) = vulkan_app.draw(|device, cmd, image| {
            drawers[active_drawer_idx].draw(device, cmd, image);
        }) {
            error!("{err}");
            exit();
        };
    });
}
