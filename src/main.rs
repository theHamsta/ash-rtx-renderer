use anyhow::Error;
use ash::vk;
use log::{error, info};
use std::{path::PathBuf, rc::Rc};

use clap::Parser;
use mesh::Mesh;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::{
    renderers::{color_sine::ColorSine, ortho::Orthographic, Renderer, RendererImpl},
    vulkan_app::VulkanApp,
};

mod mesh;
mod renderers;
mod shader;
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
    let mesh = Rc::new(Mesh::from_file(
        &args.mesh_file,
        crate::mesh::ReadOptions::OnlyTriangles,
    )?);
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

    let mut renderers = vec![
        RendererImpl::ColorSine(ColorSine::default()),
        RendererImpl::Orthographic(Orthographic::default()),
    ];
    for r in renderers.iter_mut() {
        r.set_mesh(&mesh);
    }
    let mut active_drawer_idx = 0;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        let mut exit = || *control_flow = ControlFlow::Exit;
        let mut fail = |err: Error| {
            error!("{err:?}");
            exit();
        };

        match event {
            Event::WindowEvent { event, window_id } if window_id == window.id() => match event {
                WindowEvent::CloseRequested => exit(),
                WindowEvent::Resized(size) => {
                    vulkan_app.resize(size);
                    for r in renderers.iter_mut() {
                        if let Err(err) = r.set_resolution(
                            vulkan_app.device(),
                            vulkan_app.surface_format(),
                            vk::Extent2D {
                                width: size.width,
                                height: size.height,
                            },
                            vulkan_app.images(),
                        ) {
                            fail(err)
                        };
                    }
                }
                WindowEvent::KeyboardInput { input, .. } => match input.virtual_keycode {
                    Some(winit::event::VirtualKeyCode::Escape) => {
                        renderers.drain(..);
                        exit();
                    }
                    Some(
                        winit::event::VirtualKeyCode::Numpad1 | winit::event::VirtualKeyCode::Key1,
                    ) => {
                        active_drawer_idx = 0;
                        info!(
                            "Switched Drawer to {active_drawer_idx}: {:?}",
                            renderers[active_drawer_idx]
                        );
                    }
                    Some(
                        winit::event::VirtualKeyCode::Numpad2 | winit::event::VirtualKeyCode::Key2,
                    ) => {
                        active_drawer_idx = 1;
                        info!(
                            "Switched Drawer to {active_drawer_idx}: {:?}",
                            renderers[active_drawer_idx]
                        );
                    }
                    _ => (),
                },
                _ => (),
            },
            Event::MainEventsCleared => {
                if let Err(err) = vulkan_app.draw(
                    |device, cmd, image, instant, swapchain_idx| -> Result<(), anyhow::Error> {
                        if !renderers.is_empty() {
                            renderers[active_drawer_idx].draw(
                                device,
                                cmd,
                                image,
                                instant,
                                swapchain_idx,
                            )
                        } else {
                            Ok(())
                        }
                    },
                ) {
                    renderers.drain(..);
                    fail(err)
                }
            }
            _ => (),
        }
    });
}
