use anyhow::Error;
use ash::vk;
use device_mesh::DeviceMesh;
use log::{debug, error, info, warn};
use renderers::{ray_tracing::RayTrace, RenderStyle};
use std::{
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
    renderers::{color_sine::ColorSine, raster::Raster, Renderer, RendererImpl},
    vulkan_app::VulkanApp,
};

mod acceleration_structure;
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

    #[clap(long)]
    only_triangles: bool,

    #[clap(short, long)]
    no_raytracing: bool,
}

fn main() -> anyhow::Result<()> {
    pretty_env_logger::try_init()?;

    let args = Args::parse();
    let mut meshes = Vec::new();
    for mesh in args.mesh_file.iter().map(|mesh| {
        Mesh::from_file(
            &mesh,
            if args.only_triangles {
                crate::mesh::ReadOptions::OnlyTriangles
            } else {
                crate::mesh::ReadOptions::WithAttributes
            },
        )
    }) {
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
        .with_position(winit::dpi::PhysicalPosition::new(1300i32, 800))
        .build(&event_loop)?;
    let mut with_raytracing = !args.no_raytracing;
    let mut vulkan_app = VulkanApp::new(&window, with_raytracing).or_else(|err| {
        if with_raytracing {
            error!("Failed to initialize with raytracing (is it supported by driver and hardware?). Trying again without!");
            with_raytracing = false;
            VulkanApp::new(&window, false)
        } else {
            Err(err)
        }
    })?;

    // Device must be 'static as it must outlive structs moved into eventloop referencing it
    let device = Box::leak(Box::new(vulkan_app.device().clone()));

    let raster = RendererImpl::Raster(Raster::new(device)?);
    let mut renderers = vec![raster];

    if with_raytracing {
        let raytrace = RendererImpl::RayTrace(RayTrace::new(device, vulkan_app.instance())?);
        renderers.push(raytrace);
    }
    let color_sine = RendererImpl::ColorSine(ColorSine::default());
    renderers.push(color_sine);
    debug!("Renderers: {renderers:?}");

    let meshes = meshes
        .iter()
        .map(|mesh| {
            Ok(Rc::new(DeviceMesh::new(
                device,
                vulkan_app.device_memory_properties(),
                mesh,
                with_raytracing,
            )?))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    for r in renderers.iter_mut() {
        r.set_meshes(
            &meshes,
            vulkan_app.allocate_command_buffers(1)?[0],
            vulkan_app.graphics_queue(),
            vulkan_app.device_memory_properties(),
        )?;
    }
    // Everything not moved into the event loop will not be dropped. So let renderers keep
    // references and drop manually here
    drop(meshes);

    let mut active_drawer_idx = 0;
    let mut last_switch = Instant::now();
    let mut render_style = RenderStyle::Normal;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        let mut exit = || *control_flow = ControlFlow::Exit;
        let mut fail = |err: Error| {
            error!("{err:?}");
            exit();
        };

        match event {
            Event::DeviceEvent { event, .. } => {
                for r in renderers.iter_mut() {
                    r.process_device_event(&event);
                }
            }

            Event::WindowEvent { event, window_id } if window_id == window.id() => {
                for r in renderers.iter_mut() {
                    r.process_window_event(&event);
                }
                match event {
                    WindowEvent::CloseRequested => exit(),
                    WindowEvent::Resized(size) => {
                        debug!("Resized: {size:?}");
                        vulkan_app.resize(size);
                        // Do one draw call to rebuild swapchain
                        if let Err(err) = vulkan_app.draw(
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
                                vulkan_app.surface_format(),
                                vk::Extent2D {
                                    width: size.width,
                                    height: size.height,
                                },
                                vulkan_app.images(),
                                vulkan_app.device_memory_properties(),
                                render_style,
                            ) {
                                fail(err)
                            };
                        }
                    }
                    WindowEvent::KeyboardInput { input, .. } => match input.virtual_keycode {
                        Some(
                            winit::event::VirtualKeyCode::Escape | winit::event::VirtualKeyCode::Q,
                        ) => {
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
                        Some(
                            winit::event::VirtualKeyCode::Numpad3
                            | winit::event::VirtualKeyCode::Key3,
                        ) => {
                            if renderers.len() > 2 {
                                active_drawer_idx = 2;
                                info!(
                                    "Switched Drawer to {active_drawer_idx}: {:?}",
                                    renderers[active_drawer_idx]
                                );
                            }
                        }
                        Some(
                            code @ (winit::event::VirtualKeyCode::W
                            | winit::event::VirtualKeyCode::N),
                        ) => {
                            info!("Wireframe mode",);
                            render_style = match code {
                                winit::event::VirtualKeyCode::W => RenderStyle::Wireframe,
                                winit::event::VirtualKeyCode::N => RenderStyle::Normal,
                                _ => unreachable!(),
                            };
                            for r in renderers.iter_mut() {
                                if let Err(err) = r.set_resolution(
                                    vulkan_app.surface_format(),
                                    vk::Extent2D {
                                        width: window.inner_size().width,
                                        height: window.inner_size().height,
                                    },
                                    vulkan_app.images(),
                                    vulkan_app.device_memory_properties(),
                                    render_style,
                                ) {
                                    fail(err)
                                };
                            }
                        }
                        _ => (),
                    },
                    _ => (),
                }
            }
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
                    fail(err)
                }
            }
            _ => (),
        }
    });
}
