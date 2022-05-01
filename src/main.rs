use anyhow::Error;
use ash::vk;
use device_mesh::DeviceMesh;
use log::{debug, error, info, warn};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use clap::Parser;
use mesh::Mesh;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Fullscreen, WindowBuilder},
};

use crate::{
    renderers::{color_sine::ColorSine, ortho::Orthographic, Renderer, RendererImpl},
    vulkan_app::VulkanApp,
};

mod device_mesh;
mod mesh;
mod renderers;
mod shader;
mod uniforms;
mod vulkan_app;

#[derive(clap::Parser)]
#[clap(author, version, about)]
struct Args {
    /// Mesh file to render
    #[clap(short, long)]
    mesh_file: Vec<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    pretty_env_logger::try_init()?;

    let args = Args::parse();
    let mut meshes = Vec::new();
    for mesh in args
        .mesh_file
        .iter()
        .map(|mesh| Mesh::from_file(&mesh, crate::mesh::ReadOptions::OnlyTriangles))
    {
        let mesh = mesh?;
        info!(
            "Loaded mesh with {} triangles and {} vertices. vertex_normals: {}.",
            mesh.num_triangles(),
            mesh.num_vertices(),
            mesh.has_vertex_normals()
        );
        meshes.push(Rc::new(mesh));
    }
    if meshes.is_empty() {
        warn!("No meshes specified!");
    }

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_position(winit::dpi::PhysicalPosition::new(1300, 800))
        .build(&event_loop)?;
    let vulkan_app = Rc::new(RefCell::new(VulkanApp::new(&window)?));

    let mut renderers = vec![
        RendererImpl::Orthographic(Orthographic::default()),
        RendererImpl::ColorSine(ColorSine::default()),
    ];
    let meshes = meshes
        .iter()
        .map(|mesh| {
            let vulkan_app = vulkan_app.borrow();
            Ok(Rc::new(DeviceMesh::new(
                vulkan_app.device(),
                vulkan_app.device_memory_properties(),
                mesh,
            )?))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    for r in renderers.iter_mut() {
        r.set_meshes(&meshes);
    }
    let mut active_drawer_idx = 0;
    let mut last_switch = Instant::now();

    //let mut projection_matrix =

    // original vulkan_app must be destroyed after event loop
    let vulkan_app_clone = Rc::clone(&vulkan_app);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        let mut exit = || *control_flow = ControlFlow::Exit;
        let mut fail = |err: Error| {
            error!("{err:?}");
            exit();
        };
        let mut vulkan_app_clone = vulkan_app_clone.borrow_mut();

        match event {
            Event::WindowEvent { event, window_id } if window_id == window.id() => {
                for r in renderers.iter_mut() {
                    r.process_event(&event);
                }
                match event {
                    WindowEvent::CloseRequested => exit(),
                    WindowEvent::Resized(size) => {
                        debug!("Resized: {size:?}");
                        vulkan_app_clone.resize(size);
                        // Do one draw call to rebuild swapchain
                        if let Err(err) = vulkan_app_clone.draw(
                            |_device,
                             _cmd,
                             _image,
                             _instant,
                             _swapchain_idx|
                             -> Result<(), anyhow::Error> { Ok(()) },
                        ) {
                            fail(err);
                        };
                        // Set resolution for renderers with new swapchain images
                        for r in renderers.iter_mut() {
                            if let Err(err) = r.set_resolution(
                                vulkan_app_clone.device(),
                                vulkan_app_clone.surface_format(),
                                vk::Extent2D {
                                    width: size.width,
                                    height: size.height,
                                },
                                vulkan_app_clone.images(),
                            ) {
                                fail(err)
                            };
                        }
                    }
                    WindowEvent::KeyboardInput { input, .. } => match input.virtual_keycode {
                        Some(winit::event::VirtualKeyCode::Escape) => {
                            exit();
                        }
                        Some(
                            winit::event::VirtualKeyCode::F | winit::event::VirtualKeyCode::F11,
                        ) => {
                            if (Instant::now() - last_switch) > Duration::from_millis(500) {
                                last_switch = Instant::now();
                                window.set_fullscreen(if window.fullscreen().is_some() {
                                    None
                                } else {
                                    Some(Fullscreen::Borderless(None))
                                })
                            }
                        }
                        Some(
                            winit::event::VirtualKeyCode::Numpad1
                            | winit::event::VirtualKeyCode::Key1,
                        ) => {
                            active_drawer_idx = 0;
                            info!(
                                "Switched Drawer to {active_drawer_idx}: {:?}",
                                renderers[active_drawer_idx]
                            );
                        }
                        Some(
                            winit::event::VirtualKeyCode::Numpad2
                            | winit::event::VirtualKeyCode::Key2,
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
                }
            }
            Event::MainEventsCleared => {
                if let Err(err) = vulkan_app_clone.draw(
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
                    fail(err)
                }
            }
            _ => (),
        }
    });
}
