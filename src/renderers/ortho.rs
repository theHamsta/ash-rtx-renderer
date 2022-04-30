use std::{rc::Rc, time::Instant};

use ash::vk;
use log::{debug, trace};

use crate::{device_mesh::DeviceMesh, shader::ShaderPipeline};

use super::Renderer;

#[derive(Default)]
pub struct Orthographic {
    meshes: Vec<Rc<DeviceMesh>>,
    viewports: Vec<vk::Viewport>,
    scissors: Vec<vk::Rect2D>,
    image_views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    device: Option<Rc<ash::Device>>,
    renderpass: vk::RenderPass,
    shader_pipeline: ShaderPipeline,
    pipeline: Option<vk::Pipeline>,
    resolution: vk::Rect2D,
}

impl std::fmt::Debug for Orthographic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Orthographic")
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
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        _image: vk::Image,
        _start_instant: Instant,
        swapchain_idx: usize,
    ) -> anyhow::Result<()> {
        trace!("draw for {self:?}");
        if !self.meshes.is_empty() {
            let clear_values = [
                vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 0.0],
                    },
                },
                vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                },
            ];
            if let Some(pipeline) = self.pipeline {
                let render_pass_begin_info = vk::RenderPassBeginInfo::default()
                    .render_pass(self.renderpass)
                    .framebuffer(self.framebuffers[swapchain_idx as usize])
                    .render_area(self.resolution)
                    .clear_values(&clear_values);
                unsafe {
                    device.cmd_begin_render_pass(
                        cmd,
                        &render_pass_begin_info,
                        vk::SubpassContents::INLINE,
                    );
                    device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
                    device.cmd_set_viewport(cmd, 0, &self.viewports);
                    device.cmd_set_scissor(cmd, 0, &self.scissors);
                    for mesh in self.meshes.iter() {
                        device.cmd_bind_vertex_buffers(
                            cmd,
                            0,
                            &[*mesh
                                .position()
                                .ok_or_else(|| anyhow::anyhow!("Mesh has no vertex positions"))?],
                            &[0],
                        );
                        if let Some(&idx_buffer) = mesh.indices() {
                            device.cmd_bind_index_buffer(cmd, idx_buffer, 0, vk::IndexType::UINT32);
                            device.cmd_draw_indexed(cmd, mesh.num_triangles() as u32, 1, 0, 0, 1);
                        } else {
                            device.cmd_draw(cmd, mesh.num_vertices() as u32, 1, 0, 0);
                        }
                    }
                    device.cmd_end_render_pass(cmd);
                }
            }
        }
        Ok(())
    }

    fn set_meshes(&mut self, meshes: &[Rc<DeviceMesh>]) {
        self.meshes = meshes.to_vec();
    }

    fn set_resolution(
        &mut self,
        device: &Rc<ash::Device>,
        surface_format: ash::vk::SurfaceFormatKHR,
        size: vk::Extent2D,
        images: &[vk::Image],
    ) -> anyhow::Result<()> {
        debug!("Set resolution: {size:?} images: {images:?}");
        self.destroy_images();
        self.shader_pipeline = ShaderPipeline::new(
            device,
            &[
                &include_bytes!("../../shaders/triangle.vert.spirv")[..],
                &include_bytes!("../../shaders/triangle.frag.spirv")[..],
            ],
        )?;

        self.viewports = vec![vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: size.width as f32,
            height: size.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        self.scissors = vec![size.into()];
        let vertex_attribute_desc = [];
        let vertex_binding_desc = [];
        self.pipeline = Some(self.shader_pipeline.make_graphics_pipeline(
            device,
            &self.scissors,
            &self.viewports,
            surface_format,
            &vertex_attribute_desc,
            &vertex_binding_desc,
        )?);
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
                        .map_err(|err| anyhow::anyhow!("Failed to create framebuffer: {err}"))
                }
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        self.device = Some(Rc::clone(device));
        self.resolution = size.into();
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
