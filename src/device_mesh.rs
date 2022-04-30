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

struct Buffer {
    device: Rc<ash::Device>,
    memory: vk::DeviceMemory,
    buffer: vk::Buffer,
    //buffer_view: vk::BufferView,
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            //self.device.destroy_buffer_view(self.buffer_view, None);
            self.device.free_memory(self.memory, None);
            self.device.destroy_buffer(self.buffer, None);
        }
    }
}

impl Buffer {
    fn new<T>(
        device: &Rc<ash::Device>,
        mem_properties: &vk::PhysicalDeviceMemoryProperties,
        buffer_create_info: &vk::BufferCreateInfo,
        host_memory: Option<&[T]>,
    ) -> anyhow::Result<Self>
    where
        T: Copy,
    {
        unsafe {
            let buffer = device.create_buffer(&buffer_create_info, None)?;
            let req = device.get_buffer_memory_requirements(buffer);
            let index = find_memorytype_index(
                &req,
                &mem_properties,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
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
            let device = Rc::clone(device);
            Ok(Self {
                device,
                memory,
                buffer,
            })
        }
    }
}

pub struct DeviceMesh {
    mesh: Rc<Mesh>,
    buffers: HashMap<String, Buffer>,
}

impl DeviceMesh {
    pub fn new(
        device: &Rc<ash::Device>,
        mem_properties: &vk::PhysicalDeviceMemoryProperties,
        mesh: &Rc<Mesh>,
    ) -> anyhow::Result<Self> {
        let mut buffers = HashMap::new();
        buffers.insert(
            "positions".to_owned(),
            Buffer::new(
                &device,
                mem_properties,
                &vk::BufferCreateInfo::default()
                    .size((3 * size_of::<f32>() * mesh.num_vertices()) as vk::DeviceSize)
                    .usage(vk::BufferUsageFlags::VERTEX_BUFFER)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                Some(mesh.positions()),
            )?,
        );
        buffers.insert(
            "indices".to_owned(),
            Buffer::new(
                &device,
                mem_properties,
                &vk::BufferCreateInfo::default()
                    .size((3 * size_of::<u32>() * mesh.num_triangles()) as vk::DeviceSize)
                    .usage(vk::BufferUsageFlags::INDEX_BUFFER)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                Some(mesh.triangles()),
            )?,
        );

        Ok(Self {
            mesh: Rc::clone(&mesh),
            buffers,
        })
    }
}
