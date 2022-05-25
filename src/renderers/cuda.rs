use std::{ffi::CStr, mem::MaybeUninit, ptr::null, time::Instant};

use ash::vk;

use super::Renderer;

pub struct Cuda {
    module: vk::CuModuleNVX,
    function: vk::CuFunctionNVX,
    nvx_ext: vk::NvxBinaryImportFn,
    nvx_image_view_ext: vk::NvxImageViewHandleFn,
    device: vk::Device,
}

impl std::fmt::Debug for Cuda {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cuda")
            .field("module", &self.module)
            .field("function", &self.function)
            .field("device", &self.device)
            .finish()
    }
}

impl Cuda {
    pub fn new(instance: &ash::Instance, device: vk::Device) -> anyhow::Result<Self> {
        let nvx_ext = vk::NvxBinaryImportFn::load(|name| unsafe {
            std::mem::transmute(instance.get_device_proc_addr(device, name.as_ptr()))
        });

        let nvx_image_view_ext = vk::NvxImageViewHandleFn::load(|name| unsafe {
            std::mem::transmute(instance.get_device_proc_addr(device, name.as_ptr()))
        });

        let mut module = MaybeUninit::zeroed();
        let mut function = MaybeUninit::zeroed();
        unsafe {
            let rtn = (nvx_ext.create_cu_module_nvx)(
                device,
                &vk::CuModuleCreateInfoNVX::default()
                    .data(include_bytes!("../../shaders/simple_cuda.cu.ptx")),
                null(),
                module.as_mut_ptr(),
            );
            if vk::Result::SUCCESS != rtn {
                return Err(anyhow::anyhow!("Failed to load CUDA module: {rtn}"));
            };
            let rtn = (nvx_ext.create_cu_function_nvx)(
                device,
                &vk::CuFunctionCreateInfoNVX::default()
                    .name(CStr::from_bytes_with_nul_unchecked(b"simple\0"))
                    .module(module.assume_init()),
                null(),
                function.as_mut_ptr(),
            );

            if vk::Result::SUCCESS != rtn {
                return Err(anyhow::anyhow!(
                    "Failed to load CUDA function from module: {rtn}"
                ));
            };
        }

        unsafe {
            Ok(Self {
                module: module.assume_init(),
                function: function.assume_init(),
                nvx_ext,
                nvx_image_view_ext,
                device,
            })
        }
    }
}

impl<'device> Renderer<'device> for Cuda {
    fn draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        start_instant: Instant,
        _swapchain_idx: usize,
    ) -> anyhow::Result<()> {
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

            let t = (start_instant.elapsed().as_secs_f32().sin() + 1.0) * 0.5;

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
        }
        Ok(())
    }
}
