use anyhow::Error;
use ash::vk;
use device_mesh::DeviceMesh;
use hotwatch::Hotwatch;
use log::{debug, error, info, warn};
use renderers::{ray_tracing::RayTrace, RenderStyle};
use std::{
    path::PathBuf,
    rc::Rc,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
use tracing_log::LogTracer;
use tracing_subscriber::layer::SubscriberExt;

use clap::Parser;
use mesh::Mesh;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Fullscreen, WindowBuilder},
};

use crate::{
    renderers::{color_sine::ColorSine, raster::Raster, Renderer, RendererImpl},
    vulkan_app::{TracingMode, VulkanApp},
};

mod acceleration_structure;
mod device_mesh;
mod mesh;
mod renderers;
mod shader;
mod uniforms;
mod vulkan_app;

fn setup_tracing() -> anyhow::Result<()> {
    LogTracer::init()?;
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(tracing_tracy::TracyLayer::new()),
    )
    .map_err(|err| anyhow::anyhow!("Failed to set up tracing: {err}"))
}

#[derive(clap::Parser)]
#[clap(author, version, about)]
struct Args {
    /// Mesh file to render (can be specified more than once for multiple meshes)
    #[clap(short, long)]
    mesh_file: Vec<PathBuf>,

    /// Whether to not read any Triangle attributes such as normals
    #[clap(long)]
    only_triangles: bool,

    /// Whether to disable raytracing renderer
    #[clap(short, long)]
    no_raytracing: bool,

    /// Whether to enable tracing for Tracy (https://github.com/wolfpld/tracy)
    #[clap(short, long)]
    tracing: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let tracing_mode = if args.tracing {
        setup_tracing()?;
        TracingMode::Basic
    } else {
        pretty_env_logger::try_init()?;
        TracingMode::NoTracing
    };

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
    let with_raytracing = !args.no_raytracing;
    let mut vulkan_app = VulkanApp::new(&window, with_raytracing, tracing_mode)?;

    // Device must be 'static as it must outlive structs moved into eventloop referencing it
    let device = Box::leak(Box::new(vulkan_app.device().clone()));

    let raster = RendererImpl::Raster(Raster::new(device)?);
    let mut renderers = vec![raster];

    if vulkan_app.raytracing_support() {
        let raytrace = RendererImpl::RayTrace(RayTrace::new(
            device,
            vulkan_app.instance(),
            VulkanApp::rt_pipeline_properties(
                vulkan_app.physical_device(),
                vulkan_app.instance().clone(),
            ), // hack due two weird lifetime requirements of vk::PhysicalDeviceRayTracingPipelinePropertiesKHR
        )?);
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
                vulkan_app.raytracing_support(),
            )?))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    for r in renderers.iter_mut() {
        let cmd = vulkan_app.allocate_command_buffers(1)?[0];
        r.set_meshes(
            &meshes,
            cmd,
            vulkan_app.graphics_queue(),
            vulkan_app.device_memory_properties(),
        )?;
        vulkan_app.free_command_buffers(&[cmd]);
    }
    // Everything not moved into the event loop will not be dropped. So let renderers keep
    // references and drop manually here
    drop(meshes);

    let mut active_drawer_idx = 0;
    let mut last_switch = Instant::now();
    let mut render_style = RenderStyle::Normal;
    let needs_reload = Arc::new(AtomicBool::new(false));

    let mut hotwatch = Hotwatch::new();
    if let Ok(hotwatch) = &mut hotwatch {
        for r in renderers.iter_mut() {
            for f in r
                .graphics_pipeline()
                .iter()
                .flat_map(|p| p.shaders_source_files().iter())
            {
                let needs_reload = Arc::clone(&needs_reload);
                if let Some(parent) = PathBuf::from(f).parent() {
                    let _ = hotwatch.watch(parent, move |event| match event {
                        hotwatch::Event::Create(changed) | hotwatch::Event::Write(changed) => {
                            info!("Shader file {changed:?} changed. Trying to reload");
                            needs_reload.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        _ => (),
                    });
                }
            }
        }
    }

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        let mut exit = || *control_flow = ControlFlow::Exit;
        let mut fail = |err: Error| {
            error!("{err:?}");
            exit();
        };

        if needs_reload.load(std::sync::atomic::Ordering::Relaxed) {
            for r in renderers.iter_mut() {
                warn!("trying renderer {r:?}");
                if let Some(p) = r.graphics_pipeline_mut() {
                    warn!("got the pipeline");
                    if let Err(err) = p.reload_sources() {
                        warn!("Failed to reload shaders: {err}");
                    }
                }

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
            needs_reload.store(false, std::sync::atomic::Ordering::Relaxed);
        }

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
                            winit::event::VirtualKeyCode::Numpad4
                            | winit::event::VirtualKeyCode::Key4,
                        ) => {
                            if renderers.len() > 3 {
                                active_drawer_idx = 3;
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
