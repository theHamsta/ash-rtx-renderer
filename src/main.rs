use log::info;
use std::path::PathBuf;

use clap::Parser;
use mesh::Mesh;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::vulkan_app::VulkanApp;

mod mesh;
mod vulkan_app;

#[derive(clap::Parser)]
#[clap(author, version, about)]
struct Args {
    /// Mesh file to render
    #[clap(short, long, default_value("/home/stephan/projects/ply-rs/example_plys/house_2_ok_ascii.ply".into()))]
    mesh_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    pretty_env_logger::try_init()?;

    let args = Args::parse();
    let mesh = Mesh::from_file(&args.mesh_file)?;
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
