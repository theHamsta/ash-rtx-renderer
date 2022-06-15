use anyhow::Context;
use ash::vk;
use log::trace;
use std::{
    ffi::{c_void, CStr},
    mem::MaybeUninit,
    ptr::null,
    time::Instant,
};

use super::{RenderStyle, Renderer};

pub struct Cuda<'device> {
    module: vk::CuModuleNVX,
    function: vk::CuFunctionNVX,
    nvx_ext: vk::NvxBinaryImportFn,
    nvx_image_view_ext: vk::NvxImageViewHandleFn,
    surface_format: vk::SurfaceFormatKHR,
    size: vk::Extent2D,
    sampler: vk::Sampler,
    device: &'device ash::Device,
}

impl std::fmt::Debug for Cuda<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cuda")
            .field("module", &self.module)
            .field("function", &self.function)
            .field("device", &self.device.handle())
            .finish()
    }
}

impl Drop for Cuda<'_> {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_sampler(self.sampler, None);
            (self.nvx_ext.destroy_cu_function_nvx)(self.device.handle(), self.function, null());
            (self.nvx_ext.destroy_cu_module_nvx)(self.device.handle(), self.module, null());
        }
    }
}

fn div_up(x: u32, y: u32) -> u32 {
    (x + y - 1) / y
}

impl<'device> Cuda<'device> {
    pub fn new(instance: &ash::Instance, device: &'device ash::Device) -> anyhow::Result<Self> {
        let nvx_ext = vk::NvxBinaryImportFn::load(|name| unsafe {
            std::mem::transmute(instance.get_device_proc_addr(device.handle(), name.as_ptr()))
        });

        let nvx_image_view_ext = vk::NvxImageViewHandleFn::load(|name| unsafe {
            std::mem::transmute(instance.get_device_proc_addr(device.handle(), name.as_ptr()))
        });

        let vec: Vec<u8> = include_bytes!("../../shaders/simple_cuda.cu.ptx").to_vec();

        let module = unsafe {
            let mut module = MaybeUninit::zeroed();
            (nvx_ext.create_cu_module_nvx)(
                device.handle(),
                &vk::CuModuleCreateInfoNVX::default().data(&vec[..]),
                null(),
                module.as_mut_ptr(),
            )
            .result_with_success(module.assume_init())
            .context("Failed to load CUDA module")?
        };
        let function = unsafe {
            let mut function = MaybeUninit::zeroed();
            (nvx_ext.create_cu_function_nvx)(
                device.handle(),
                &vk::CuFunctionCreateInfoNVX::default()
                    .name(CStr::from_bytes_with_nul_unchecked(b"simple\0"))
                    .module(module),
                null(),
                function.as_mut_ptr(),
            )
            .result_with_success(function.assume_init())
            .context("Failed to load CUDA function")?
        };

        let sampler = unsafe { device.create_sampler(&vk::SamplerCreateInfo::default(), None)? };

        Ok(Self {
            module,
            function,
            nvx_ext,
            nvx_image_view_ext,
            device,
            surface_format: vk::SurfaceFormatKHR::default().format(vk::Format::R8G8B8A8_SNORM),
            size: vk::Extent2D {
                width: 0,
                height: 0,
            },
            sampler,
        })
    }
}

impl<'device> Renderer<'device> for Cuda<'device> {
    fn draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        start_instant: Instant,
        _swapchain_idx: usize,
    ) -> anyhow::Result<()> {
        trace!("Draw!");

        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::default(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .image(image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })],
            );

            let create_view_info = vk::ImageViewCreateInfo::default()
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(self.surface_format.format)
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

            let time = (start_instant.elapsed().as_secs_f32().sin() + 1.0) * 0.5;
            let vk::Extent2D { height, width } = self.size;

            let block_x = 16;
            let block_y = 16;

            let image_view = device.create_image_view(&create_view_info, None)?;
            let handle_info = vk::ImageViewHandleInfoNVX::default()
                .image_view(image_view)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .sampler(self.sampler);
            let surface =
                (self.nvx_image_view_ext.get_image_view_handle_nvx)(device.handle(), &handle_info);

            trace!("Launch CUDA kernel");
            (self.nvx_ext.cmd_cu_launch_kernel_nvx)(
                cmd,
                &vk::CuLaunchInfoNVX::default()
                    .function(self.function)
                    .grid_dim_x(div_up(width, block_x))
                    .grid_dim_y(div_up(height, block_y))
                    .grid_dim_z(1)
                    .block_dim_x(block_x)
                    .block_dim_y(block_y)
                    .block_dim_z(1)
                    .shared_mem_bytes(0)
                    .params(&[
                        (&width) as *const u32 as *const c_void,
                        (&height) as *const u32 as *const c_void,
                        (&time) as *const f32 as *const c_void,
                        (&surface) as *const u32 as *const c_void,
                    ]),
            );

            // Typically this barrier would be implemented with the implicit subpass dependency to
            // EXTERNAL
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::default(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                    .image(image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })],
            );
            device.destroy_image_view(image_view, None);
        }
        Ok(())
    }

    fn set_resolution(
        &mut self,
        surface_format: ash::vk::SurfaceFormatKHR,
        size: vk::Extent2D,
        _images: &[vk::Image],
        _device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
        _render_style: RenderStyle,
    ) -> anyhow::Result<()> {
        self.surface_format = surface_format;
        self.size = size;
        Ok(())
    }
}
