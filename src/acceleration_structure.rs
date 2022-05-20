use std::mem::size_of;

use anyhow::Context;
use ash::vk;

use crate::{
    device_mesh::{Buffer, DeviceMesh},
    mesh::Position,
};

pub struct AccelerationStructureData<'device> {
    structure: vk::AccelerationStructureKHR,
    buffer: Buffer<'device>,
    handle: vk::DeviceAddress,
}

impl<'device> AccelerationStructureData<'device> {
    fn build_bottomlevel(
        cmd: vk::CommandBuffer,
        mesh: &'device DeviceMesh,
        device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
        as_extension: &ash::extensions::khr::AccelerationStructure,
        graphics_queue: vk::Queue,
    ) -> anyhow::Result<AccelerationStructureData<'device>> {
        let device = mesh.device();
        let geometry = vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
            .geometry(vk::AccelerationStructureGeometryDataKHR {
                triangles: vk::AccelerationStructureGeometryTrianglesDataKHR::default()
                    .vertex_data(vk::DeviceOrHostAddressConstKHR {
                        device_address: unsafe {
                            mesh.device().get_buffer_device_address(
                                &vk::BufferDeviceAddressInfo::default().buffer(
                                    *mesh.position().ok_or_else(|| {
                                        anyhow::anyhow!("No vertex buffer on mesh")
                                    })?,
                                ),
                            )
                        },
                    })
                    .max_vertex(mesh.num_vertices() as u32 - 1)
                    .vertex_stride(size_of::<Position>() as u64)
                    .vertex_format(vk::Format::R32G32B32_SFLOAT)
                    .index_data(vk::DeviceOrHostAddressConstKHR {
                        device_address: unsafe {
                            mesh.device().get_buffer_device_address(
                                &vk::BufferDeviceAddressInfo::default().buffer(
                                    *mesh.indices().ok_or_else(|| {
                                        anyhow::anyhow!("No index buffer on mesh")
                                    })?,
                                ),
                            )
                        },
                    })
                    .index_type(vk::IndexType::UINT32),
            })
            .flags(vk::GeometryFlagsKHR::OPAQUE);
        let build_range_info = vk::AccelerationStructureBuildRangeInfoKHR::default()
            .first_vertex(0)
            .primitive_count(mesh.num_triangles() as u32 / 3)
            .primitive_offset(0)
            .transform_offset(0);

        let geometries = &[geometry];
        let mut build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
            .geometries(geometries)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL);

        let size_info = unsafe {
            as_extension.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_info,
                &[mesh.num_triangles() as u32],
            )
        };

        let mut bottom_as_buffer = Buffer::new::<u8>(
            device,
            device_memory_properties,
            &vk::BufferCreateInfo::default()
                .size(size_info.acceleration_structure_size)
                .usage(
                    vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                        | vk::BufferUsageFlags::STORAGE_BUFFER,
                ),
            None,
        )?;

        let as_create_info = vk::AccelerationStructureCreateInfoKHR::default()
            .ty(build_info.ty)
            .size(size_info.acceleration_structure_size)
            .buffer(*bottom_as_buffer.buffer_mut())
            .offset(0);

        let bottom_as =
            unsafe { as_extension.create_acceleration_structure(&as_create_info, None) }.unwrap();

        build_info.dst_acceleration_structure = bottom_as;

        let mut scratch_buffer = Buffer::new::<u8>(
            &device,
            device_memory_properties,
            &vk::BufferCreateInfo::default()
                .size(size_info.build_scratch_size)
                .usage(
                    vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                        | vk::BufferUsageFlags::STORAGE_BUFFER,
                ),
            None,
        )?;

        build_info.scratch_data = vk::DeviceOrHostAddressKHR {
            device_address: unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(*scratch_buffer.buffer_mut()),
                )
            },
        };
        unsafe {
            device
                .begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .unwrap();

            as_extension.cmd_build_acceleration_structures(
                cmd,
                &[build_info],
                &[&[build_range_info]],
            );
            device.end_command_buffer(cmd)?;
            device
                .queue_submit(
                    graphics_queue,
                    &[vk::SubmitInfo::default().command_buffers(&[cmd])],
                    vk::Fence::null(),
                )
                .context("queue submit failed.")?;

            device.queue_wait_idle(graphics_queue).unwrap();
        }

        let handle = unsafe {
            as_extension.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                    .acceleration_structure(bottom_as),
            )
        };
        Ok(AccelerationStructureData {
            buffer: bottom_as_buffer,
            structure: bottom_as,
            handle,
        })
    }

    pub fn reference(&self) -> vk::AccelerationStructureReferenceKHR {
        vk::AccelerationStructureReferenceKHR {
            device_handle: self.handle,
        }
    }

    pub fn build_toplevel(
        cmd: vk::CommandBuffer,
        instances: &'device [&(&AccelerationStructureData, [f32; 12])],
        device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
        as_extension: &ash::extensions::khr::AccelerationStructure,
        graphics_queue: vk::Queue,
    ) -> anyhow::Result<Self> {
        let device = instances[0].0.device();
        let instances: Vec<_> = instances
            .iter()
            .map(
                |(bottomlevel_as, transform)| vk::AccelerationStructureInstanceKHR {
                    transform: vk::TransformMatrixKHR { matrix: *transform },
                    instance_shader_binding_table_record_offset_and_flags: ash::vk::Packed24_8::new(
                        0,
                        vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw() as u8,
                    ),
                    instance_custom_index_and_mask: ash::vk::Packed24_8::new(0, 0xff),
                    acceleration_structure_reference: bottomlevel_as.reference(),
                },
            )
            .collect();
        let instance_buffer_size =
            std::mem::size_of::<vk::AccelerationStructureInstanceKHR>() * instances.len();

        let mut instance_buffer = Buffer::new(
            &device,
            device_memory_properties,
            &vk::BufferCreateInfo::default()
                .size(instance_buffer_size as vk::DeviceSize)
                .usage(
                    vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                        | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
                ),
            Some(&instances),
        )?;
        let (top_as, top_as_buffer) = {
            let build_range_info = vk::AccelerationStructureBuildRangeInfoKHR::default()
                .first_vertex(0)
                .primitive_count(instances.len() as u32)
                .primitive_offset(0)
                .transform_offset(0);

            unsafe {
                device
                    .begin_command_buffer(
                        cmd,
                        &vk::CommandBufferBeginInfo::default()
                            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                    )
                    .unwrap();
                let memory_barrier = vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR);
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                    vk::DependencyFlags::empty(),
                    &[memory_barrier],
                    &[],
                    &[],
                );
            }

            let instances = vk::AccelerationStructureGeometryInstancesDataKHR::default()
                .array_of_pointers(false)
                .data(vk::DeviceOrHostAddressConstKHR {
                    device_address: unsafe {
                        device.get_buffer_device_address(
                            &vk::BufferDeviceAddressInfo::default()
                                .buffer(*instance_buffer.buffer_mut()),
                        )
                    },
                });

            let geometry = vk::AccelerationStructureGeometryKHR::default()
                .geometry_type(vk::GeometryTypeKHR::INSTANCES)
                .geometry(vk::AccelerationStructureGeometryDataKHR { instances });

            let geometries = [geometry];

            let mut build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
                .geometries(&geometries)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL);

            let size_info = unsafe {
                as_extension.get_acceleration_structure_build_sizes(
                    vk::AccelerationStructureBuildTypeKHR::DEVICE,
                    &build_info,
                    &[build_range_info.primitive_count],
                )
            };

            let mut top_as_buffer = Buffer::new::<u8>(
                &device,
                device_memory_properties,
                &vk::BufferCreateInfo::default()
                    .size(size_info.acceleration_structure_size)
                    .usage(
                        vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                            | vk::BufferUsageFlags::STORAGE_BUFFER,
                    ),
                None,
            )?;

            let as_create_info = vk::AccelerationStructureCreateInfoKHR::default()
                .ty(build_info.ty)
                .size(size_info.acceleration_structure_size)
                .buffer(*top_as_buffer.buffer_mut())
                .offset(0);

            let top_as =
                unsafe { as_extension.create_acceleration_structure(&as_create_info, None) }
                    .unwrap();

            build_info.dst_acceleration_structure = top_as;

            let mut scratch_buffer = Buffer::new::<u8>(
                &device,
                device_memory_properties,
                &vk::BufferCreateInfo::default()
                    .size(size_info.build_scratch_size)
                    .usage(
                        vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                            | vk::BufferUsageFlags::STORAGE_BUFFER,
                    ),
                None,
            )?;

            build_info.scratch_data = vk::DeviceOrHostAddressKHR {
                device_address: unsafe {
                    device.get_buffer_device_address(
                        &vk::BufferDeviceAddressInfo::default()
                            .buffer(*scratch_buffer.buffer_mut()),
                    )
                },
            };

            unsafe {
                as_extension.cmd_build_acceleration_structures(
                    cmd,
                    &[build_info],
                    &[&[build_range_info]],
                );
                device.end_command_buffer(cmd).unwrap();
                device
                    .queue_submit(
                        graphics_queue,
                        &[vk::SubmitInfo::default().command_buffers(&[cmd])],
                        vk::Fence::null(),
                    )
                    .expect("queue submit failed.");

                device.queue_wait_idle(graphics_queue).unwrap();
            }

            (top_as, top_as_buffer)
        };

        Ok(Self {
            structure: top_as,
            buffer: top_as_buffer,
            handle: unsafe {
                as_extension.get_acceleration_structure_device_address(
                    &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                        .acceleration_structure(top_as),
                )
            },
        })
    }

    pub fn device(&self) -> &ash::Device {
        self.buffer.device()
    }
}
