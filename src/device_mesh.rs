//
// device_mesh.rs
// Copyright (C) 2022 Stephan Seitz <stephan.seitz@fau.de>
// Distributed under terms of the GPLv3 license.
//

use std::{
    collections::HashMap,
    mem::{align_of, size_of},
    rc::Rc,
};

use ash::{util::Align, vk};
use log::debug;

use crate::mesh::Mesh;

// From ash examples
fn find_memorytype_index(
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

pub struct Buffer<'device> {
    device: &'device ash::Device,
    memory: vk::DeviceMemory,
    buffer: vk::Buffer,
    //buffer_view: vk::BufferView,
}

impl Drop for Buffer<'_> {
    fn drop(&mut self) {
        unsafe {
            //self.device.destroy_buffer_view(self.buffer_view, None);
            self.device.free_memory(self.memory, None);
            self.device.destroy_buffer(self.buffer, None);
        }
    }
}

impl<'device> Buffer<'device> {
    pub fn new<T>(
        device: &'device ash::Device,
        mem_properties: &vk::PhysicalDeviceMemoryProperties,
        buffer_create_info: &vk::BufferCreateInfo,
        host_memory: Option<&[T]>,
    ) -> anyhow::Result<Self>
    where
        T: Copy,
    {
        debug!("allocating memory: {:?}", buffer_create_info);
        unsafe {
            let buffer = device.create_buffer(buffer_create_info, None)?;
            let req = device.get_buffer_memory_requirements(buffer);
            let index = find_memorytype_index(
                &req,
                mem_properties,
                if host_memory.is_some() {
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT
                } else {
                    vk::MemoryPropertyFlags::DEVICE_LOCAL
                },
            )
            .ok_or_else(|| anyhow::anyhow!("Failed to get memory index"))?;
            let memory = device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(index),
                None,
            )?;
            if let Some(host_memory) = host_memory {
                let ptr = device.map_memory(memory, 0, req.size, vk::MemoryMapFlags::empty())?;
                let mut map_slice = Align::new(ptr, align_of::<T>() as u64, req.size);
                map_slice.copy_from_slice(host_memory);
                device.unmap_memory(memory);
            }
            device.bind_buffer_memory(buffer, memory, 0)?;
            Ok(Self {
                device,
                memory,
                buffer,
            })
        }
    }

    /// Get the buffer's device.
    #[must_use]
    pub fn device(&self) -> &ash::Device {
        self.device
    }

    /// Get a mutable reference to the buffer's buffer.
    #[must_use]
    pub fn buffer_mut(&mut self) -> &mut vk::Buffer {
        &mut self.buffer
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum AttributeType {
    Normals,
    Position,
    Index,
}

pub struct DeviceMesh<'device> {
    mesh: Rc<Mesh>,
    buffers: HashMap<AttributeType, Buffer<'device>>,
    device: &'device ash::Device,
}

impl<'device> DeviceMesh<'device> {
    pub fn new(
        device: &'device ash::Device,
        mem_properties: &vk::PhysicalDeviceMemoryProperties,
        mesh: &Rc<Mesh>,
        with_ray_tracing: bool,
    ) -> anyhow::Result<Self> {
        let mut buffers = HashMap::new();
        let vertex_buffer_usage = if with_ray_tracing {
            vk::BufferUsageFlags::VERTEX_BUFFER
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
        } else {
            vk::BufferUsageFlags::VERTEX_BUFFER
        };
        let index_buffer_usage = if with_ray_tracing {
            vk::BufferUsageFlags::INDEX_BUFFER
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
        } else {
            vk::BufferUsageFlags::INDEX_BUFFER
        };
        buffers.insert(
            AttributeType::Position,
            Buffer::new(
                device,
                mem_properties,
                &vk::BufferCreateInfo::default()
                    .size((3 * size_of::<f32>() * mesh.num_vertices()) as vk::DeviceSize)
                    .usage(vertex_buffer_usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                Some(mesh.positions()),
            )?,
        );
        if let Some(vertex_normals) = mesh.vertex_normals() {
            buffers.insert(
                AttributeType::Normals,
                Buffer::new(
                    device,
                    mem_properties,
                    &vk::BufferCreateInfo::default()
                        .size((3 * size_of::<f32>() * mesh.num_vertices()) as vk::DeviceSize)
                        .usage(vertex_buffer_usage)
                        .sharing_mode(vk::SharingMode::EXCLUSIVE),
                    Some(vertex_normals),
                )?,
            );
        }
        buffers.insert(
            AttributeType::Index,
            Buffer::new(
                device,
                mem_properties,
                &vk::BufferCreateInfo::default()
                    .size((3 * size_of::<u32>() * mesh.num_triangles()) as vk::DeviceSize)
                    .usage(index_buffer_usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                Some(mesh.triangles()),
            )?,
        );

        Ok(Self {
            mesh: Rc::clone(mesh),
            buffers,
            device,
        })
    }

    pub fn position(&self) -> Option<&vk::Buffer> {
        self.buffers
            .get(&AttributeType::Position)
            .map(|b| &b.buffer)
    }

    pub fn indices(&self) -> Option<&vk::Buffer> {
        self.buffers.get(&AttributeType::Index).map(|b| &b.buffer)
    }

    pub fn normals(&self) -> Option<&vk::Buffer> {
        self.buffers.get(&AttributeType::Normals).map(|b| &b.buffer)
    }

    pub fn num_triangles(&self) -> usize {
        self.mesh.num_triangles()
    }

    pub fn num_vertices(&self) -> usize {
        self.mesh.num_vertices()
    }

    /// Get a reference to the device mesh's mesh.
    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        self.mesh.as_ref()
    }

    /// Get the device mesh's device.
    #[must_use]
    pub fn device(&self) -> &ash::Device {
        self.device
    }
}
