use crate::{
    acceleration_structure::{BottomLevelAccelerationStructure, TopLevelAccelerationStructure},
    device_mesh::Buffer,
};
use std::{
    io::{Cursor, Write},
    mem::size_of,
    rc::Rc,
    time::Instant,
};

use ash::vk::{self, ShaderStageFlags};
use cgmath::{Point3, Vector3, Vector4};
use log::{debug, trace};
use winit::event::WindowEvent;

use crate::{device_mesh::DeviceMesh, shader::ShaderPipeline, uniforms::PushConstants};

use super::{RenderStyle, Renderer};

pub struct RayTrace<'device> {
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
    descriptor_set: Option<vk::DescriptorSet>,
    descriptor_pool: Option<vk::DescriptorPool>,
    sbt: Option<Buffer<'device>>,
}

static NUM_ATTRIBUTES: usize = 2;

impl<'device> RayTrace<'device> {
    pub fn new(
        device: &'device ash::Device,
        instance: &ash::Instance,
        rt_pipeline_properties: vk::PhysicalDeviceRayTracingPipelinePropertiesKHR<'device>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            zoom: 1.0,
            image_views: Default::default(),
            device,
            shader_pipeline: ShaderPipeline::new(
                device,
                &[
                    &include_bytes!("../../shaders/raygen.glsl.spirv")[..],
                    &include_bytes!("../../shaders/miss.glsl.spirv")[..],
                    &include_bytes!("../../shaders/closest_hit.glsl.spirv")[..],
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
            descriptor_set: None,
            descriptor_pool: None,
        })
    }

    fn num_instances(&self) -> u32 {
        self.toplevel_as
            .as_ref()
            .map(|a| a.bottomlevel_as().len() as u32)
            .unwrap_or(0)
    }

    fn destroy_descriptor_sets(&mut self) {
        unsafe {
            if let Some(pool) = self.descriptor_pool.take() {
                self.descriptor_set
                    .take()
                    .map(|l| self.device.free_descriptor_sets(pool, &[l]));
                self.device.destroy_descriptor_pool(pool, None);
            }
        }
    }
}

impl std::fmt::Debug for RayTrace<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RayTrace")
            .field("image_views", &self.image_views)
            .finish()
    }
}

impl<'device> RayTrace<'device> {
    fn destroy_images(&mut self) -> anyhow::Result<()> {
        if let Some(p) = self.pipeline_layout.take() {
            unsafe { self.device.destroy_pipeline_layout(p, None) }
        }
        if let Some(p) = self.pipeline.take() {
            unsafe { self.device.destroy_pipeline(p, None) };
        }
        unsafe {
            let device = self.device;
            device.device_wait_idle()?;
            for img in self.image_views.drain(..) {
                device.destroy_image_view(img, None);
            }
        }
        Ok(())
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
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        _image: vk::Image,
        _start_instant: Instant,
        swapchain_idx: usize,
    ) -> anyhow::Result<()> {
        trace!("draw for {self:?}");
        if self.toplevel_as.is_some() {
            let accel_structs = [self
                .toplevel_as
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("No toplevel acceleration structure"))?
                .structure()];
            let mut accel_info = vk::WriteDescriptorSetAccelerationStructureKHR::default()
                .acceleration_structures(&accel_structs);

            let mut accel_write = vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_set.unwrap())
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .push_next(&mut accel_info);

            // This is only set by the builder for images, buffers, or views; need to set explicitly after
            accel_write.descriptor_count = 1;

            let image_info = [vk::DescriptorImageInfo::default()
                .image_layout(vk::ImageLayout::GENERAL)
                .image_view(self.image_views[swapchain_idx])];

            // TODO: Probably the image should be a PushConstant
            let image_write = vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_set.unwrap())
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(&image_info);

            unsafe {
                trace!("update_descriptor_sets");
                device.update_descriptor_sets(&[accel_write, image_write], &[]);
            }

            {
                let sbt_address = self.sbt.as_ref().unwrap().device_address();

                let aligned_size = aligned_size(
                    self.rt_pipeline_properties.shader_group_handle_size,
                    self.rt_pipeline_properties.shader_group_base_alignment,
                ) as u64;
                let sbt_raygen_region = vk::StridedDeviceAddressRegionKHR::default()
                    .device_address(sbt_address)
                    .size(self.rt_pipeline_properties.shader_group_handle_size.into())
                    .stride(aligned_size);

                let sbt_miss_region = vk::StridedDeviceAddressRegionKHR::default()
                    .device_address(sbt_address + aligned_size)
                    .size(aligned_size)
                    .stride(self.rt_pipeline_properties.shader_group_handle_size.into());

                let sbt_hit_region = vk::StridedDeviceAddressRegionKHR::default()
                    .device_address(sbt_address + 2 * aligned_size)
                    .size(aligned_size * self.num_instances() as u64)
                    .stride(self.rt_pipeline_properties.shader_group_handle_size.into());

                let sbt_call_region = vk::StridedDeviceAddressRegionKHR::default();

                unsafe {
                    device.cmd_bind_pipeline(
                        cmd,
                        vk::PipelineBindPoint::RAY_TRACING_KHR,
                        self.pipeline.unwrap(),
                    );
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::RAY_TRACING_KHR,
                        self.pipeline_layout.unwrap(),
                        0,
                        &[self.descriptor_set.unwrap()],
                        &[],
                    );

                    self.device.cmd_push_constants(
                        cmd,
                        self.pipeline_layout.unwrap(),
                        vk::ShaderStageFlags::RAYGEN_KHR,
                        0,
                        &std::mem::transmute::<PushConstants, [u8; size_of::<PushConstants>()]>(
                            self.uniforms.unwrap(),
                        ),
                    );
                    trace!("cmd_trace_rays");
                    self.raytracing_tracing_ext.cmd_trace_rays(
                        cmd,
                        &sbt_raygen_region,
                        &sbt_miss_region,
                        &sbt_hit_region,
                        &sbt_call_region,
                        self.size.width,
                        self.size.height,
                        1,
                    );
                }
            }
        }
        Ok(())
    }

    fn set_meshes(
        &mut self,
        meshes: &[Rc<DeviceMesh<'device>>],
        cmd: vk::CommandBuffer,
        graphics_queue: vk::Queue,
        device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
    ) -> anyhow::Result<()> {
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
        let bottomlevel_as = meshes
            .iter()
            .enumerate()
            .flat_map(|(i, m)| {
                Some((
                    BottomLevelAccelerationStructure::build_bottomlevel(
                        cmd,
                        self.device,
                        Rc::clone(m),
                        device_memory_properties,
                        &self.acceleration_structure_ext,
                        graphics_queue,
                    )
                    .ok()?,
                    [
                        1.0,
                        0.0,
                        0.0,
                        100.0 * i as f32,
                        0.0,
                        1.0 + i as f32,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        1.0,
                        0.0,
                    ],
                ))
            })
            .collect();
        self.toplevel_as = Some(TopLevelAccelerationStructure::build_toplevel(
            cmd,
            self.device,
            bottomlevel_as,
            device_memory_properties,
            self.acceleration_structure_ext.clone(),
            graphics_queue,
            NUM_ATTRIBUTES as u32,
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
        self.destroy_images()?;
        self.destroy_descriptor_sets();
        self.update_push_constants();
        self.size = size;

        let mut shader_groups = vec![
            // raygen
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(0)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
        ];
        shader_groups.push(
            // miss
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(1)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
        );
        for _ in 0..self.num_instances() {
            shader_groups.push(
                // closest
                vk::RayTracingShaderGroupCreateInfoKHR::default()
                    .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
                    .general_shader(vk::SHADER_UNUSED_KHR)
                    .closest_hit_shader(2)
                    .any_hit_shader(vk::SHADER_UNUSED_KHR)
                    .intersection_shader(vk::SHADER_UNUSED_KHR),
            );
        }

        let descriptor_set_layout = unsafe {
            let binding_flags_inner = [
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
                    ])
                    .push_next(&mut binding_flags),
                None,
            )
        }?;
        let max_recursion_depth = 1;

        let (pipeline, pipeline_layout) = self.shader_pipeline.make_rtx_pipeline(
            device,
            &shader_groups,
            &self.raytracing_tracing_ext,
            descriptor_set_layout,
            max_recursion_depth,
            &[vk::PushConstantRange::default()
                .offset(0)
                .size(size_of::<PushConstants>().try_into()?)
                .stage_flags(ShaderStageFlags::RAYGEN_KHR | ShaderStageFlags::CLOSEST_HIT_KHR)],
        )?;

        let sbt = {
            let handle_size = self.rt_pipeline_properties.shader_group_handle_size;
            let raygen_data = unsafe {
                self.raytracing_tracing_ext
                    .get_ray_tracing_shader_group_handles(pipeline, 0, 1, handle_size as usize)
            }?;

            let missdata = unsafe {
                self.raytracing_tracing_ext
                    .get_ray_tracing_shader_group_handles(pipeline, 1, 1, handle_size as usize)
            }?;

            let chit_data = unsafe {
                self.raytracing_tracing_ext
                    .get_ray_tracing_shader_group_handles(
                        pipeline,
                        2,
                        self.num_instances(),
                        handle_size as usize * self.num_instances() as usize,
                    )
            }?;

            let table_size = aligned_size(
                raygen_data.len() as u32,
                self.rt_pipeline_properties.shader_group_base_alignment,
            ) + self.num_instances()
                * aligned_size(
                    self.rt_pipeline_properties.shader_group_handle_size as u32
                        + 2 * NUM_ATTRIBUTES as u32,
                    self.rt_pipeline_properties.shader_group_base_alignment,
                )
                + aligned_size(
                    missdata.len() as u32,
                    self.rt_pipeline_properties.shader_group_base_alignment,
                );
            let mut table_data = vec![0u8; table_size as usize];
            let mut cur = Cursor::new(&mut table_data);
            let mut written = 0;
            written += cur.write(&raygen_data)?;
            written = aligned_size(
                written as u32,
                self.rt_pipeline_properties.shader_group_base_alignment,
            ) as usize;
            cur.set_position(written as u64);

            written += cur.write(&missdata)?;
            written = aligned_size(
                written as u32,
                self.rt_pipeline_properties.shader_group_base_alignment,
            ) as usize;
            cur.set_position(written as u64);

            for (i, mesh) in self
                .toplevel_as
                .as_ref()
                .unwrap()
                .meshes()
                .iter()
                .enumerate()
            {
                written += cur.write(
                    &chit_data[i * self.rt_pipeline_properties.shader_group_handle_size as usize
                        ..((i + 1)
                            * self.rt_pipeline_properties.shader_group_handle_size as usize)],
                )?;
                written += cur.write(
                    &mesh
                        .indices_device_address()
                        .ok_or_else(|| anyhow::anyhow!("No indices found on mesh"))?
                        .to_le_bytes(),
                )?;
                written += cur.write(
                    &mesh
                        .normals_device_address()
                        .ok_or_else(|| anyhow::anyhow!("No normals found on mesh"))?
                        .to_le_bytes(),
                )?;
                written = aligned_size(
                    written as u32,
                    self.rt_pipeline_properties.shader_group_base_alignment,
                ) as usize;
                cur.set_position(written as u64);
            }
            assert_eq!(written, table_size as usize);

            Buffer::new(
                device,
                device_memory_properties,
                &vk::BufferCreateInfo::default()
                    .size(table_size as u64)
                    .usage(vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS),
                Some(&table_data),
            )?
        };

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
        }?;

        unsafe {
            self.device
                .destroy_descriptor_set_layout(descriptor_set_layout, None)
        };

        let descriptor_set = descriptor_sets[0];
        self.descriptor_set = Some(descriptor_set);
        self.descriptor_pool = Some(descriptor_pool);

        self.resolution = size.into();
        Ok(())
    }

    fn graphics_pipeline(&self) -> Option<&ShaderPipeline> {
        Some(&self.shader_pipeline)
    }

    fn graphics_pipeline_mut(&mut self) -> Option<&mut ShaderPipeline<'device>> {
        Some(&mut self.shader_pipeline)
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
        let _ = self.destroy_images();
        self.destroy_descriptor_sets();
    }
}

fn aligned_size(value: u32, alignment: u32) -> u32 {
    (value + alignment - 1) & !(alignment - 1)
}
