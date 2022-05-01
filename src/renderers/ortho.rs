#[warn(unused_unsafe)]
use std::{mem::size_of, mem::transmute, rc::Rc, time::Instant};

use ash::vk;
use cgmath::Vector4;
use log::{debug, trace};
use winit::event::WindowEvent;

use crate::{
    device_mesh::DeviceMesh, mesh::Position, shader::ShaderPipeline, uniforms::PushConstants,
};

use super::Renderer;

pub struct Orthographic {
    meshes: Vec<Rc<DeviceMesh>>,
    viewports: Vec<vk::Viewport>,
    scissors: Vec<vk::Rect2D>,
    image_views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    device: Option<Rc<ash::Device>>,
    renderpass: Option<vk::RenderPass>,
    shader_pipeline: ShaderPipeline,
    pipeline: Option<vk::Pipeline>,
    pipeline_layout: Option<vk::PipelineLayout>,
    resolution: vk::Rect2D,
    uniforms: Option<PushConstants>,
    size: vk::Extent2D,
    zoom: f32,
    rotation: f32,
}

impl Default for Orthographic {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            meshes: Default::default(),
            viewports: Default::default(),
            scissors: Default::default(),
            image_views: Default::default(),
            framebuffers: Default::default(),
            device: Default::default(),
            renderpass: Default::default(),
            shader_pipeline: Default::default(),
            pipeline: Default::default(),
            pipeline_layout: Default::default(),
            resolution: Default::default(),
            uniforms: None,
            size: vk::Extent2D {
                width: 0,
                height: 0,
            },
            rotation: 0.0,
        }
    }
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
    fn update_push_constants(&mut self) {
        self.uniforms = Some(PushConstants::new(
            self.size,
            Vector4::new(2.0, 0.0, 0.0, 1.0),
            self.zoom,
            self.rotation,
        ));
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
                //vk::ClearValue {
                //depth_stencil: vk::ClearDepthStencilValue {
                //depth: 1.0,
                //stencil: 0,
                //},
                //},
            ];
            if let Some(pipeline) = self.pipeline {
                let render_pass_begin_info = vk::RenderPassBeginInfo::default()
                    .render_pass(
                        self.renderpass
                            .ok_or_else(|| anyhow::anyhow!("No renderpass created"))?,
                    )
                    .framebuffer(self.framebuffers[swapchain_idx as usize])
                    .render_area(self.resolution)
                    .clear_values(&clear_values);
                trace!("{render_pass_begin_info:?}");
                unsafe {
                    device.cmd_begin_render_pass(
                        cmd,
                        &render_pass_begin_info,
                        vk::SubpassContents::INLINE,
                    );
                    device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
                    device.cmd_set_viewport(cmd, 0, &self.viewports);
                    device.cmd_set_scissor(cmd, 0, &self.scissors);

                    device.cmd_push_constants(
                        cmd,
                        self.pipeline_layout.unwrap(),
                        vk::ShaderStageFlags::VERTEX,
                        0,
                        &transmute::<PushConstants, [u8; size_of::<PushConstants>()]>(
                            self.uniforms.unwrap(),
                        ),
                    );
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
        self.size = size;
        self.update_push_constants();
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
        let vertex_attribute_desc = [vk::VertexInputAttributeDescription {
            location: 0,
            binding: 0,
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: 0,
        }];
        let vertex_binding_desc = [vk::VertexInputBindingDescription {
            binding: 0,
            stride: std::mem::size_of::<Position>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }];
        let (pipeline, renderpass, pipeline_layout) = self.shader_pipeline.make_graphics_pipeline(
            device,
            &self.scissors,
            &self.viewports,
            surface_format,
            &vertex_attribute_desc,
            &vertex_binding_desc,
        )?;
        self.renderpass = Some(renderpass);
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
        self.framebuffers = self
            .image_views
            .iter()
            .map(|&view| {
                let framebuffer_attachments = [view /* base.depth_image_view*/];
                let frame_buffer_create_info = vk::FramebufferCreateInfo::default()
                    .render_pass(renderpass)
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

    fn process_event(&mut self, event: &winit::event::WindowEvent) {
        let mut handled = true;
        match event {
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

impl Drop for Orthographic {
    fn drop(&mut self) {
        self.destroy_images();
    }
}
