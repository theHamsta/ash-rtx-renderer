use std::{rc::Rc, time::Instant};

use ash::vk;
use log::trace;

use crate::{mesh::Mesh, shader::ShaderPipeline};

use super::Renderer;

#[derive(Default)]
pub struct Orthographic {
    meshes: Vec<Rc<Mesh>>,
    viewports: Vec<vk::Viewport>,
    scissors: Vec<vk::Rect2D>,
    image_views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    device: Option<ash::Device>,
    renderpass: vk::RenderPass,
    shader_pipeline: ShaderPipeline,
}

impl std::fmt::Debug for Orthographic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Orthographic")
            .field("mesh", &self.meshes)
            .field("viewports", &self.viewports)
            .field("scissors", &self.scissors)
            .field("image_views", &self.image_views)
            .field("framebuffers", &self.framebuffers)
            .finish()
    }
}

impl Orthographic {
    fn destroy_images(&mut self) {
        unsafe {
            if let Some(device) = &self.device {
                device.device_wait_idle().unwrap();
            }
            for img in self.image_views.iter() {
                self.device.as_ref().unwrap().destroy_image_view(*img, None);
            }
            for img in self.framebuffers.iter() {
                self.device
                    .as_ref()
                    .unwrap()
                    .destroy_framebuffer(*img, None);
            }
        }
    }
}

impl Renderer for Orthographic {
    fn draw(
        &self,
        _device: &ash::Device,
        _cmd: vk::CommandBuffer,
        _image: vk::Image,
        _start_instant: Instant,
        _swapchain_idx: usize,
    ) -> anyhow::Result<()> {
        trace!("draw for {self:?}");
        if !self.meshes.is_empty() {
            //unsafe {
            //device.cmd_begin_render_pass(
            //cmd,
            //&render_pass_begin_info,
            //vk::SubpassContents::INLINE,
            //);
            //device.cmd_bind_pipeline(
            //cmd,
            //vk::PipelineBindPoint::GRAPHICS,
            //self.graphic_pipeline,
            //);
            //device.cmd_set_viewport(cmd, 0, &self.viewports);
            //device.cmd_set_scissor(cmd, 0, &self.scissors);
            //device.cmd_bind_vertex_buffers(cmd, 0, &[self.vertex_input_buffer], &[0]);
            //device.cmd_bind_index_buffer(cmd, self.index_buffer, 0, vk::IndexType::UINT32);
            //device.cmd_draw_indexed(cmd, self.index_buffer_data.len() as u32, 1, 0, 0, 1);
            //// Or draw without the index buffer
            //// device.cmd_draw(draw_command_buffer, 3, 1, 0, 0);
            //device.cmd_end_render_pass(cmd);
            //}
            //todo!();
        }
        Ok(())
    }

    fn set_meshes(&mut self, meshes: &[Rc<Mesh>]) {
        self.meshes = meshes.iter().cloned().collect();
    }

    fn set_resolution(
        &mut self,
        device: &ash::Device,
        surface_format: ash::vk::SurfaceFormatKHR,
        size: vk::Extent2D,
        images: &[vk::Image],
    ) -> anyhow::Result<()> {
        self.destroy_images();
        self.shader_pipeline = ShaderPipeline::new(
            device,
            &[
                &include_bytes!("../../shaders/triangle.vert.spirv")[..],
                &include_bytes!("../../shaders/triangle.frag.spirv")[..],
            ],
        )?;

        let renderpass_attachments = [
            vk::AttachmentDescription {
                format: surface_format.format,
                samples: vk::SampleCountFlags::TYPE_1,
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::STORE,
                final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                ..Default::default()
            },
            //vk::AttachmentDescription {
            //format: vk::Format::D16_UNORM,
            //samples: vk::SampleCountFlags::TYPE_1,
            //load_op: vk::AttachmentLoadOp::CLEAR,
            //initial_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            //final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            //..Default::default()
            //},
        ];
        let color_attachment_refs = [vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];
        //let depth_attachment_ref = vk::AttachmentReference {
        //attachment: 1,
        //layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        //};
        let dependencies = [vk::SubpassDependency {
            src_subpass: vk::SUBPASS_EXTERNAL,
            src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_READ
                | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            ..Default::default()
        }];

        let subpass = vk::SubpassDescription::default()
            .color_attachments(&color_attachment_refs)
            //.depth_stencil_attachment(&depth_attachment_ref)
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS);

        let renderpass_create_info = vk::RenderPassCreateInfo::default()
            .attachments(&renderpass_attachments)
            .subpasses(std::slice::from_ref(&subpass))
            .dependencies(&dependencies);

        self.renderpass = unsafe { device.create_render_pass(&renderpass_create_info, None)? };
        self.viewports = vec![vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: size.width as f32,
            height: size.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        self.scissors = vec![size.into()];
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
                    .render_pass(self.renderpass)
                    .attachments(&framebuffer_attachments)
                    .width(size.width)
                    .height(size.height)
                    .layers(1);

                unsafe {
                    device
                        .create_framebuffer(&frame_buffer_create_info, None)
                        .unwrap()
                }
            })
            .collect();

        self.device = Some(device.clone());
        Ok(())
    }

    fn graphics_pipeline(&self) -> Option<&ShaderPipeline> {
        Some(&self.shader_pipeline)
    }
}

impl Drop for Orthographic {
    fn drop(&mut self) {
        self.destroy_images();
    }
}
