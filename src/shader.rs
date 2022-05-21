use std::ffi::{CStr, CString};
use std::intrinsics::transmute;
use std::io::Cursor;

use ash::vk::{VertexInputAttributeDescription, VertexInputBindingDescription};
use ash::{util::read_spv, vk};
use log::debug;

use crate::renderers::RenderStyle;

pub struct Shader {
    module: vk::ShaderModule,
    info: spirv_reflect::ShaderModule,
}

pub struct ShaderPipeline<'device> {
    shaders: Vec<Shader>,
    device: &'device ash::Device,
}

impl Drop for ShaderPipeline<'_> {
    fn drop(&mut self) {
        for s in self.shaders.iter() {
            unsafe { self.device.destroy_shader_module(s.module, None) };
        }
    }
}

impl<'device> ShaderPipeline<'device> {
    pub fn new(device: &'device ash::Device, shader_bytes: &[&[u8]]) -> anyhow::Result<Self> {
        let mut shaders = Vec::new();
        for &bytes in shader_bytes {
            let info = spirv_reflect::ShaderModule::load_u8_data(bytes)
                .map_err(|err| anyhow::anyhow!("{err}"))?;
            debug!(
                "Loaded shader {:?} ({:?}) in: {:?}, out: {:?} _push_constant_blocks {:?}",
                info.get_source_file(),
                info.get_shader_stage(),
                info.enumerate_input_variables(None),
                info.enumerate_output_variables(None),
                info.enumerate_push_constant_blocks(None)
            );

            shaders.push(Shader {
                module: unsafe {
                    device.create_shader_module(
                        &vk::ShaderModuleCreateInfo::default()
                            .code(&read_spv(&mut Cursor::new(bytes))?),
                        None,
                    )?
                },
                info,
                //alt_info,
            });
        }
        Ok(Self { shaders, device })
    }

    pub fn make_graphics_pipeline(
        &self,
        device: &ash::Device,
        scissors: &[vk::Rect2D],
        viewports: &[vk::Viewport],
        surface_format: vk::SurfaceFormatKHR,
        vertex_input_attribute_descriptions: &[VertexInputAttributeDescription],
        vertex_input_binding_descriptions: &[VertexInputBindingDescription],
        push_constant_ranges: &[vk::PushConstantRange],
        render_style: RenderStyle,
    ) -> anyhow::Result<(vk::Pipeline, vk::RenderPass, vk::PipelineLayout)> {
        let shader_entry_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"main\0") };
        let shader_stage_create_infos = self
            .shaders
            .iter()
            .map(|shader| {
                vk::PipelineShaderStageCreateInfo::default()
                    .name(shader_entry_name)
                    .module(shader.module)
                    .stage(unsafe { transmute(shader.info.get_shader_stage()) })
            })
            .collect::<Vec<_>>();

        let vertex_input_state_info = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_attribute_descriptions(vertex_input_attribute_descriptions)
            .vertex_binding_descriptions(vertex_input_binding_descriptions);
        let vertex_input_assembly_state_info = vk::PipelineInputAssemblyStateCreateInfo {
            topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            ..Default::default()
        };
        let rasterization_info = vk::PipelineRasterizationStateCreateInfo {
            front_face: vk::FrontFace::COUNTER_CLOCKWISE,
            line_width: 1.0,
            polygon_mode: match render_style {
                RenderStyle::Normal => vk::PolygonMode::FILL,
                RenderStyle::Wireframe => vk::PolygonMode::LINE,
            },
            cull_mode: vk::CullModeFlags::BACK,
            ..Default::default()
        };
        let multisample_state_info = vk::PipelineMultisampleStateCreateInfo {
            rasterization_samples: vk::SampleCountFlags::TYPE_1,
            ..Default::default()
        };
        let noop_stencil_state = vk::StencilOpState {
            fail_op: vk::StencilOp::KEEP,
            pass_op: vk::StencilOp::KEEP,
            depth_fail_op: vk::StencilOp::KEEP,
            compare_op: vk::CompareOp::ALWAYS,
            ..Default::default()
        };
        let depth_state_info = vk::PipelineDepthStencilStateCreateInfo {
            depth_test_enable: 1,
            depth_write_enable: 1,
            depth_compare_op: vk::CompareOp::LESS_OR_EQUAL,
            front: noop_stencil_state,
            back: noop_stencil_state,
            max_depth_bounds: 1.0,
            ..Default::default()
        };
        let color_blend_attachment_states = [vk::PipelineColorBlendAttachmentState {
            blend_enable: 0,
            src_color_blend_factor: vk::BlendFactor::SRC_COLOR,
            dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_DST_COLOR,
            color_blend_op: vk::BlendOp::ADD,
            src_alpha_blend_factor: vk::BlendFactor::ZERO,
            dst_alpha_blend_factor: vk::BlendFactor::ZERO,
            alpha_blend_op: vk::BlendOp::ADD,
            color_write_mask: vk::ColorComponentFlags::RGBA,
        }];
        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op(vk::LogicOp::CLEAR)
            .attachments(&color_blend_attachment_states);

        let viewport_state_info = vk::PipelineViewportStateCreateInfo::default()
            .scissors(scissors)
            .viewports(viewports);
        let dynamic_state = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let color_attachment_refs = [vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];
        let depth_attachment_ref = vk::AttachmentReference {
            attachment: 1,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        };

        let subpass = vk::SubpassDescription::default()
            .color_attachments(&color_attachment_refs)
            .depth_stencil_attachment(&depth_attachment_ref)
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS);

        let renderpass_attachments = [
            vk::AttachmentDescription {
                format: surface_format.format,
                samples: vk::SampleCountFlags::TYPE_1,
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::STORE,
                initial_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                ..Default::default()
            },
            vk::AttachmentDescription {
                format: vk::Format::D16_UNORM,
                samples: vk::SampleCountFlags::TYPE_1,
                load_op: vk::AttachmentLoadOp::CLEAR,
                initial_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                ..Default::default()
            },
        ];

        let dependencies = [vk::SubpassDependency {
            src_subpass: vk::SUBPASS_EXTERNAL,
            src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_READ
                | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            ..Default::default()
        }];

        let renderpass_create_info = vk::RenderPassCreateInfo::default()
            .attachments(&renderpass_attachments)
            .subpasses(std::slice::from_ref(&subpass))
            .dependencies(&dependencies);

        let renderpass = unsafe { device.create_render_pass(&renderpass_create_info, None)? };

        let dynamic_state_info =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_state);

        let layout_create_info =
            vk::PipelineLayoutCreateInfo::default().push_constant_ranges(push_constant_ranges);

        let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_create_info, None)? };
        Ok((
            unsafe {
                device.create_graphics_pipelines(
                    vk::PipelineCache::null(), // TODO:: create cache
                    &[vk::GraphicsPipelineCreateInfo::default()
                        .stages(&shader_stage_create_infos)
                        .vertex_input_state(&vertex_input_state_info)
                        .input_assembly_state(&vertex_input_assembly_state_info)
                        .viewport_state(&viewport_state_info)
                        .rasterization_state(&rasterization_info)
                        .multisample_state(&multisample_state_info)
                        .depth_stencil_state(&depth_state_info)
                        .color_blend_state(&color_blend_state)
                        .dynamic_state(&dynamic_state_info)
                        .layout(pipeline_layout)
                        .render_pass(renderpass)],
                    None,
                )
            }
            .map_err(|(_pipes, err)| err)?[0],
            renderpass,
            pipeline_layout,
        ))
    }

    pub fn make_rtx_pipeline(
        &self,
        device: &ash::Device,
        shader_groups: &[vk::RayTracingShaderGroupCreateInfoKHR],
        raytracing_ext: &ash::extensions::khr::RayTracingPipeline,
        descriptor_set_layout: vk::DescriptorSetLayout,
        max_pipeline_ray_recursion_depth: u32,
        push_constant_ranges: &[vk::PushConstantRange],
    ) -> anyhow::Result<(vk::Pipeline, vk::PipelineLayout)> {
        let layouts = vec![descriptor_set_layout];
        let layout_create_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&layouts)
            .push_constant_ranges(push_constant_ranges);

        let pipeline_layout =
            unsafe { device.create_pipeline_layout(&layout_create_info, None) }.unwrap();

        let shader_stage_create_infos = self
            .shaders
            .iter()
            .map(|shader| {
                vk::PipelineShaderStageCreateInfo::default()
                    .name(unsafe { CStr::from_bytes_with_nul_unchecked(b"main\0") })
                    .module(shader.module)
                    .stage(unsafe { transmute(shader.info.get_shader_stage()) })
            })
            .collect::<Vec<_>>();
        let pipeline = unsafe {
            raytracing_ext.create_ray_tracing_pipelines(
                vk::DeferredOperationKHR::null(),
                vk::PipelineCache::null(),
                &[vk::RayTracingPipelineCreateInfoKHR::default()
                    .stages(&shader_stage_create_infos)
                    .groups(shader_groups)
                    .max_pipeline_ray_recursion_depth(max_pipeline_ray_recursion_depth)
                    .layout(pipeline_layout)],
                None,
            )
        }?[0];

        Ok((pipeline, pipeline_layout))
    }
}
