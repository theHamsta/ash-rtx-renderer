use crate::mesh::{Normal, Position};
use std::{mem::size_of, mem::transmute, rc::Rc, time::Instant};

use ash::vk::{self, ShaderStageFlags};
use cgmath::{Point3, Vector3, Vector4};
use log::{debug, trace};
use winit::event::WindowEvent;

use crate::{device_mesh::DeviceMesh, shader::ShaderPipeline, uniforms::PushConstants};

use super::{RenderStyle, Renderer};

pub fn find_memorytype_index(
    memory_req: &vk::MemoryRequirements,
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_prop.memory_types[..memory_prop.memory_type_count as _]
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            (1 << index) & memory_req.memory_type_bits != 0
                && memory_type.property_flags & flags == flags
        })
        .map(|(index, _memory_type)| index as _)
}

pub struct Orthographic<'device> {
    meshes: Vec<Rc<DeviceMesh<'device>>>,
    viewports: Vec<vk::Viewport>,
    scissors: Vec<vk::Rect2D>,
    image_views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    device: &'device ash::Device,
    renderpass: Option<vk::RenderPass>,
    shader_pipeline: ShaderPipeline<'device>,
    pipeline: Option<vk::Pipeline>,
    pipeline_layout: Option<vk::PipelineLayout>,
    resolution: vk::Rect2D,
    depth_image: vk::Image,
    depth_image_view: vk::ImageView,
    depth_image_memory: vk::DeviceMemory,
    uniforms: Option<PushConstants>,
    size: vk::Extent2D,
    zoom: f32,
    rotation: f32,
    translation: Point3<f32>,
    middle_drag: bool,
}

impl<'device> Orthographic<'device> {
    pub fn new(device: &'device ash::Device) -> anyhow::Result<Self> {
        Ok(Self {
            zoom: 1.0,
            meshes: Default::default(),
            viewports: Default::default(),
            scissors: Default::default(),
            image_views: Default::default(),
            framebuffers: Default::default(),
            device,
            renderpass: Default::default(),
            shader_pipeline: ShaderPipeline::new(
                device,
                &[
                    &include_bytes!("../../shaders/triangle.vert.spirv")[..],
                    &include_bytes!("../../shaders/triangle.frag.spirv")[..],
                ],
            )?,
            translation: Point3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            pipeline: Default::default(),
            pipeline_layout: Default::default(),
            resolution: Default::default(),
            depth_image: Default::default(),
            depth_image_view: Default::default(),
            depth_image_memory: Default::default(),
            uniforms: None,
            size: vk::Extent2D {
                width: 0,
                height: 0,
            },
            rotation: 0.0,
            middle_drag: false,
        })
    }
}

impl std::fmt::Debug for Orthographic<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Orthographic")
            .field("viewports", &self.viewports)
            .field("scissors", &self.scissors)
            .field("image_views", &self.image_views)
            .field("framebuffers", &self.framebuffers)
            .finish()
    }
}

impl<'device> Orthographic<'device> {
    fn destroy_images(&mut self) {
        unsafe {
            let device = self.device;
            device.device_wait_idle().unwrap();
            device.destroy_image(self.depth_image, None);
            device.destroy_image_view(self.depth_image_view, None);
            device.free_memory(self.depth_image_memory, None);
            for img in self.image_views.iter() {
                device.destroy_image_view(*img, None);
            }
            for img in self.framebuffers.iter() {
                device.destroy_framebuffer(*img, None);
            }
        }
    }
    fn update_push_constants(&mut self) {
        self.uniforms = Some(PushConstants::new(
            self.size,
            self.translation,
            Vector4::new(2.0, 0.0, 0.0, 1.0),
            self.zoom,
            self.rotation,
        ));
    }
}

impl<'device> Renderer<'device> for Orthographic<'device> {
    fn draw(
        &self,
        _device: &ash::Device,
        cmd: vk::CommandBuffer,
        _image: vk::Image,
        _start_instant: Instant,
        swapchain_idx: usize,
    ) -> anyhow::Result<()> {
        trace!("draw for {self:?}");
        if !self.meshes.is_empty() {
            let clear_values = [
                vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 0.0],
                    },
                },
                vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                },
            ];
            if let Some(pipeline) = self.pipeline {
                let render_pass_begin_info = vk::RenderPassBeginInfo::default()
                    .render_pass(
                        self.renderpass
                            .ok_or_else(|| anyhow::anyhow!("No renderpass created"))?,
                    )
                    .framebuffer(self.framebuffers[swapchain_idx as usize])
                    .render_area(self.resolution)
                    .clear_values(&clear_values);
                trace!("{render_pass_begin_info:?}");
                unsafe {
                    self.device.cmd_begin_render_pass(
                        cmd,
                        &render_pass_begin_info,
                        vk::SubpassContents::INLINE,
                    );
                    self.device
                        .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
                    self.device.cmd_set_viewport(cmd, 0, &self.viewports);
                    self.device.cmd_set_scissor(cmd, 0, &self.scissors);

                    self.device.cmd_push_constants(
                        cmd,
                        self.pipeline_layout.unwrap(),
                        vk::ShaderStageFlags::VERTEX,
                        0,
                        &transmute::<PushConstants, [u8; size_of::<PushConstants>()]>(
                            self.uniforms.unwrap(),
                        ),
                    );
                    let device = self.device;
                    for mesh in self.meshes.iter() {
                        device.cmd_bind_vertex_buffers(
                            cmd,
                            0,
                            &[
                                *mesh.position().ok_or_else(|| {
                                    anyhow::anyhow!("Mesh has no vertex positions")
                                })?,
                                *mesh
                                    .normals()
                                    .ok_or_else(|| anyhow::anyhow!("Mesh has no vertex normals"))?,
                            ],
                            &[0, 0],
                        );
                        if let Some(&idx_buffer) = mesh.indices() {
                            device.cmd_bind_index_buffer(cmd, idx_buffer, 0, vk::IndexType::UINT32);
                            device.cmd_draw_indexed(
                                cmd,
                                mesh.num_triangles() as u32 * 3,
                                1,
                                0,
                                0,
                                1,
                            );
                        } else {
                            device.cmd_draw(cmd, mesh.num_vertices() as u32 * 3, 1, 0, 0);
                        }
                    }
                    device.cmd_end_render_pass(cmd);
                }
            }
        }
        Ok(())
    }

    fn set_meshes(&mut self, meshes: &[Rc<DeviceMesh<'device>>]) {
        self.meshes = meshes.to_vec();
        self.translation = meshes
            .iter()
            .flat_map(|mesh| mesh.mesh().positions().iter())
            .fold(
                Point3 {
                    x: 0.0f32,
                    y: 0.0,
                    z: 0.0,
                },
                |i, &p| {
                    i + Vector3 {
                        x: p.x,
                        y: p.y,
                        z: p.z,
                    }
                },
            )
            / meshes.iter().map(|mesh| mesh.num_vertices()).sum::<usize>() as f32;
    }

    fn set_resolution(
        &mut self,
        surface_format: ash::vk::SurfaceFormatKHR,
        size: vk::Extent2D,
        images: &[vk::Image],
        device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
        render_style: RenderStyle,
    ) -> anyhow::Result<()> {
        let device = self.device;
        debug!("Set resolution: {size:?} images: {images:?}");
        self.destroy_images();
        self.size = size;
        self.update_push_constants();

        self.viewports = vec![vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: size.width as f32,
            height: size.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        self.scissors = vec![size.into()];
        let vertex_attribute_desc = [
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 0,
            },
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 1,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 0,
            },
        ];
        let vertex_binding_desc = [
            vk::VertexInputBindingDescription {
                binding: 0,
                stride: std::mem::size_of::<Position>() as u32,
                input_rate: vk::VertexInputRate::VERTEX,
            },
            vk::VertexInputBindingDescription {
                binding: 1,
                stride: std::mem::size_of::<Normal>() as u32,
                input_rate: vk::VertexInputRate::VERTEX,
            },
        ];
        let (pipeline, renderpass, pipeline_layout) = self.shader_pipeline.make_graphics_pipeline(
            device,
            &self.scissors,
            &self.viewports,
            surface_format,
            &vertex_attribute_desc,
            &vertex_binding_desc,
            &[vk::PushConstantRange::default()
                .offset(0)
                .size(size_of::<PushConstants>().try_into()?)
                .stage_flags(ShaderStageFlags::VERTEX)],
            render_style,
        )?;
        self.renderpass = Some(renderpass);
        self.pipeline = Some(pipeline);
        self.pipeline_layout = Some(pipeline_layout);
        self.image_views = images
            .iter()
            .map(|&image| {
                let create_view_info = vk::ImageViewCreateInfo::default()
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(surface_format.format)
                    .components(vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    })
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .image(image);
                unsafe { device.create_image_view(&create_view_info, None).unwrap() }
            })
            .collect();

        let depth_image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::D16_UNORM)
            .extent(vk::Extent3D {
                width: size.width,
                height: size.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        self.depth_image = unsafe { device.create_image(&depth_image_create_info, None)? };

        self.depth_image_memory = unsafe {
            let depth_image_memory_req = device.get_image_memory_requirements(self.depth_image);
            let depth_image_memory_index = find_memorytype_index(
                &depth_image_memory_req,
                device_memory_properties,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )
            .ok_or_else(|| anyhow::anyhow!("Could not find memory index for depth buffer"))?;
            let depth_image_allocate_info = vk::MemoryAllocateInfo::default()
                .allocation_size(depth_image_memory_req.size)
                .memory_type_index(depth_image_memory_index);

            device.allocate_memory(&depth_image_allocate_info, None)?
        };
        unsafe { device.bind_image_memory(self.depth_image, self.depth_image_memory, 0)? };
        self.depth_image_view = unsafe {
            let depth_image_view_info = vk::ImageViewCreateInfo::default()
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::DEPTH)
                        .level_count(1)
                        .layer_count(1),
                )
                .image(self.depth_image)
                .format(depth_image_create_info.format)
                .view_type(vk::ImageViewType::TYPE_2D);

            device.create_image_view(&depth_image_view_info, None)?
        };

        self.framebuffers = self
            .image_views
            .iter()
            .map(|&view| {
                let framebuffer_attachments = [view, self.depth_image_view];
                let frame_buffer_create_info = vk::FramebufferCreateInfo::default()
                    .render_pass(renderpass)
                    .attachments(&framebuffer_attachments)
                    .width(size.width)
                    .height(size.height)
                    .layers(1);

                unsafe {
                    device
                        .create_framebuffer(&frame_buffer_create_info, None)
                        .map_err(|err| anyhow::anyhow!("Failed to create framebuffer: {err}"))
                }
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        self.resolution = size.into();
        Ok(())
    }

    fn graphics_pipeline(&self) -> Option<&ShaderPipeline> {
        Some(&self.shader_pipeline)
    }

    fn process_device_event(&mut self, event: &winit::event::DeviceEvent) {
        #[allow(clippy::single_match)]
        match event {
            winit::event::DeviceEvent::MouseMotion { delta } => {
                if self.middle_drag {
                    self.translation -= Vector3 {
                        x: self.rotation.cos() * delta.0 as f32,
                        y: delta.1 as f32,
                        z: self.rotation.sin() * delta.0 as f32,
                    };
                }
            }
            _ => (),
        }
    }
    fn process_window_event(&mut self, event: &winit::event::WindowEvent) {
        let mut handled = true;
        match event {
            WindowEvent::MouseInput { state, button, .. } => match (button, state) {
                //(winit::event::MouseButton::Left, winit::event::ElementState::Pressed) => todo!(),
                //(winit::event::MouseButton::Left, winit::event::ElementState::Released) => todo!(),
                //(winit::event::MouseButton::Right, winit::event::ElementState::Pressed) => todo!(),
                //(winit::event::MouseButton::Right, winit::event::ElementState::Released) => todo!(),
                (winit::event::MouseButton::Middle, winit::event::ElementState::Pressed) => {
                    self.middle_drag = true
                }
                (winit::event::MouseButton::Middle, winit::event::ElementState::Released) => {
                    self.middle_drag = false
                }
                _ => (),
            },
            WindowEvent::MouseWheel { delta, .. } => match delta {
                winit::event::MouseScrollDelta::LineDelta(_h, v) => self.zoom += 0.1 * v,
                winit::event::MouseScrollDelta::PixelDelta(_) => (),
            },
            WindowEvent::KeyboardInput { input, .. } => match input.virtual_keycode {
                Some(winit::event::VirtualKeyCode::Left) => self.rotation += 5.0,
                Some(winit::event::VirtualKeyCode::Right) => self.rotation -= 5.0,
                Some(winit::event::VirtualKeyCode::Down) => self.zoom += 0.1,
                Some(winit::event::VirtualKeyCode::Up) => self.zoom -= 0.1,
                _ => handled = false,
            },
            _ => handled = false,
        }
        if handled {
            self.update_push_constants();
        }
    }
}

impl Drop for Orthographic<'_> {
    fn drop(&mut self) {
        self.destroy_images();
    }
}
