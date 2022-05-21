use crate::{
    acceleration_structure::{AccelerationStructureData, TopLevelAccelerationStructure},
    mesh::{Normal, Position},
};
use std::{mem::size_of, rc::Rc, time::Instant};

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

pub struct RayTrace<'device> {
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
    uniforms: Option<PushConstants>,
    size: vk::Extent2D,
    zoom: f32,
    rotation: f32,
    translation: Point3<f32>,
    middle_drag: bool,
    toplevel_as: Option<TopLevelAccelerationStructure<'device>>,
    raytracing_tracing_ext: ash::extensions::khr::RayTracingPipeline,
    acceleration_structure_ext: ash::extensions::khr::AccelerationStructure,
}

impl<'device> RayTrace<'device> {
    pub fn new(device: &'device ash::Device, instance: &ash::Instance) -> anyhow::Result<Self> {
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
            uniforms: None,
            size: vk::Extent2D {
                width: 0,
                height: 0,
            },
            rotation: 0.0,
            middle_drag: false,
            toplevel_as: Default::default(),
            acceleration_structure_ext: ash::extensions::khr::AccelerationStructure::new(
                instance, device,
            ),
            raytracing_tracing_ext: ash::extensions::khr::RayTracingPipeline::new(instance, device),
        })
    }
}

impl std::fmt::Debug for RayTrace<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Orthographic")
            .field("viewports", &self.viewports)
            .field("scissors", &self.scissors)
            .field("image_views", &self.image_views)
            .field("framebuffers", &self.framebuffers)
            .finish()
    }
}

impl<'device> RayTrace<'device> {
    fn destroy_images(&mut self) {
        unsafe {
            let device = self.device;
            device.device_wait_idle().unwrap();
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

impl<'device> Renderer<'device> for RayTrace<'device> {
    fn draw(
        &self,
        _device: &ash::Device,
        _cmd: vk::CommandBuffer,
        _image: vk::Image,
        _start_instant: Instant,
        _swapchain_idx: usize,
    ) -> anyhow::Result<()> {
        trace!("draw for {self:?}");
        if !self.meshes.is_empty() {}
        Ok(())
    }

    fn set_meshes(
        &mut self,
        meshes: &[Rc<DeviceMesh<'device>>],
        cmd: vk::CommandBuffer,
        graphics_queue: vk::Queue,
        device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
    ) -> anyhow::Result<()> {
        self.translation = self
            .meshes
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
        let bottomlevel_as = meshes
            .iter()
            .flat_map(|m| {
                Some((AccelerationStructureData::build_bottomlevel(
                    cmd,
                    self.device,
                    Rc::clone(m),
                    device_memory_properties,
                    &self.acceleration_structure_ext,
                    graphics_queue,
                ).ok()?, [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]))
            })
            .collect();
        self.toplevel_as = Some(TopLevelAccelerationStructure::build_toplevel(
            cmd,
            self.device,
            bottomlevel_as,
            device_memory_properties,
            &self.acceleration_structure_ext,
            graphics_queue,
        )?);
        Ok(())
    }

    fn set_resolution(
        &mut self,
        surface_format: ash::vk::SurfaceFormatKHR,
        size: vk::Extent2D,
        images: &[vk::Image],
        _device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
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

        self.framebuffers = self
            .image_views
            .iter()
            .map(|&view| {
                let framebuffer_attachments = [view];
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

impl Drop for RayTrace<'_> {
    fn drop(&mut self) {
        self.destroy_images();
    }
}
