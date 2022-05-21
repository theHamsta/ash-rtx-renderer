use crate::{
    acceleration_structure::{AccelerationStructureData, TopLevelAccelerationStructure},
    device_mesh::Buffer,
};
use std::{mem::size_of, rc::Rc, time::Instant};

use ash::vk::{self, ShaderStageFlags};
use cgmath::{Point3, Vector3, Vector4};
use log::{debug, trace};
use winit::event::WindowEvent;

use crate::{device_mesh::DeviceMesh, shader::ShaderPipeline, uniforms::PushConstants};

use super::{RenderStyle, Renderer};

pub struct RayTrace<'device> {
    meshes: Vec<Rc<DeviceMesh<'device>>>,
    image_views: Vec<vk::ImageView>,
    device: &'device ash::Device,
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
    rt_pipeline_properties: vk::PhysicalDeviceRayTracingPipelinePropertiesKHR<'device>,
    sbt: Option<Buffer<'device>>,
}

impl<'device> RayTrace<'device> {
    pub fn new(
        device: &'device ash::Device,
        instance: &ash::Instance,
        rt_pipeline_properties: vk::PhysicalDeviceRayTracingPipelinePropertiesKHR<'device>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            zoom: 1.0,
            meshes: Default::default(),
            image_views: Default::default(),
            device,
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
            rt_pipeline_properties,
            sbt: None,
        })
    }
}

impl std::fmt::Debug for RayTrace<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Orthographic")
            .field("image_views", &self.image_views)
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
                Some((
                    AccelerationStructureData::build_bottomlevel(
                        cmd,
                        self.device,
                        Rc::clone(m),
                        device_memory_properties,
                        &self.acceleration_structure_ext,
                        graphics_queue,
                    )
                    .ok()?,
                    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                ))
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
        device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
        _render_style: RenderStyle,
    ) -> anyhow::Result<()> {
        let device = self.device;
        debug!("Set resolution: {size:?} images: {images:?}");
        self.destroy_images();
        self.size = size;
        self.update_push_constants();

        let shader_groups = vec![
            // group0 = [ raygen ]
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(0)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
            // group1 = [ chit ]
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
                .general_shader(vk::SHADER_UNUSED_KHR)
                .closest_hit_shader(1)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
            // group2 = [ miss ]
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(2)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
        ];

        let descriptor_set_layout = unsafe {
            let binding_flags_inner = [
                vk::DescriptorBindingFlagsEXT::empty(),
                vk::DescriptorBindingFlagsEXT::empty(),
                vk::DescriptorBindingFlagsEXT::empty(),
            ];

            let mut binding_flags = vk::DescriptorSetLayoutBindingFlagsCreateInfoEXT::default()
                .binding_flags(&binding_flags_inner);
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default()
                    .bindings(&[
                        vk::DescriptorSetLayoutBinding::default()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                            .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR)
                            .binding(0),
                        vk::DescriptorSetLayoutBinding::default()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                            .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR)
                            .binding(1),
                        vk::DescriptorSetLayoutBinding::default()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .stage_flags(vk::ShaderStageFlags::CLOSEST_HIT_KHR)
                            .binding(2),
                        vk::DescriptorSetLayoutBinding::default()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .stage_flags(vk::ShaderStageFlags::CLOSEST_HIT_KHR)
                            .binding(3),
                    ])
                    .push_next(&mut binding_flags),
                None,
            )
        }
        .unwrap();
        let max_recursion_depth = 1;

        let (pipeline, pipeline_layout, sbt) = self.shader_pipeline.make_rtx_pipeline(
            device,
            &shader_groups,
            &self.raytracing_tracing_ext,
            descriptor_set_layout,
            max_recursion_depth,
            device_memory_properties,
            &self.rt_pipeline_properties,
            &[vk::PushConstantRange::default()
                .offset(0)
                .size(size_of::<PushConstants>().try_into()?)
                .stage_flags(ShaderStageFlags::VERTEX)],
        )?;
        self.sbt = Some(sbt);
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

        let descriptor_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                descriptor_count: 1,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_IMAGE,
                descriptor_count: 1,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 1,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 1,
            },
        ];

        let descriptor_pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&descriptor_sizes)
            .max_sets(1);

        let descriptor_pool =
            unsafe { device.create_descriptor_pool(&descriptor_pool_info, None) }?;

        let mut count_allocate_info =
            vk::DescriptorSetVariableDescriptorCountAllocateInfo::default().descriptor_counts(&[1]);

        let descriptor_sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&[descriptor_set_layout])
                    .push_next(&mut count_allocate_info),
            )
        }
        .unwrap();

        let descriptor_set = descriptor_sets[0];

        let accel_structs = [self.toplevel_as.handle()];
        let mut accel_info = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&accel_structs);

        let mut accel_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
            .push_next(&mut accel_info);

        // This is only set by the builder for images, buffers, or views; need to set explicitly after
        accel_write.descriptor_count = 1;

        let image_info = [vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::GENERAL)
            .image_view(self.image_views[0])];

        let image_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .image_info(&image_info);

        let buffer_info = [vk::DescriptorBufferInfo::default()
            .buffer()
            .range(vk::WHOLE_SIZE)];

        let buffers_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(2)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&buffer_info);

        unsafe {
            device.update_descriptor_sets(&[accel_write, image_write, buffers_write], &[]);
        }

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
