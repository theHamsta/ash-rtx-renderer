use std::{rc::Rc, time::Instant};

use ash::vk;
use log::trace;

use crate::mesh::Mesh;

use super::Renderer;

#[derive(Default)]
pub struct Orthographic {
    mesh: Option<Rc<Mesh>>,
    viewports: Vec<vk::Viewport>,
    scissors: Vec<vk::Rect2D>,
    image_views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    device: Option<ash::Device>,
}

impl std::fmt::Debug for Orthographic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Orthographic")
            .field("mesh", &self.mesh)
            .field("viewports", &self.viewports)
            .field("scissors", &self.scissors)
            .field("image_views", &self.image_views)
            .field("framebuffers", &self.framebuffers)
            .finish()
    }
}

impl Renderer for Orthographic {
    fn draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        start_instant: Instant,
    ) -> anyhow::Result<()> {
        trace!("draw for {self:?}");
        if let Some(mesh) = &self.mesh {
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
            todo!();
        }
        Ok(())
    }

    fn set_mesh(&mut self, mesh: Rc<Mesh>) {
        self.mesh = Some(mesh);
    }

    fn set_resolution(
        &mut self,
        device: &ash::Device,
        surface_format: ash::vk::SurfaceFormatKHR,
        size: vk::Extent2D,
        images: &[vk::Image],
    ) {
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

        self.device = Some(device.clone());
    }
}

impl Drop for Orthographic {
    fn drop(&mut self) {
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
