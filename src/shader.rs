use std::io::Cursor;

use ash::{util::read_spv, vk};
use log::debug;

pub struct Shader {
    module: vk::ShaderModule,
    info: spirv_reflect::ShaderModule,
}

#[derive(Default)]
pub struct ShaderPipeline {
    shaders: Vec<Shader>,
}

impl ShaderPipeline {
    pub fn new(device: &ash::Device, shader_bytes: &[&[u8]]) -> anyhow::Result<Self> {
        let mut shaders = Vec::new();
        for &bytes in shader_bytes {
            let info = spirv_reflect::ShaderModule::load_u8_data(bytes)
                .map_err(|err| anyhow::anyhow!("{err}"))?;
            debug!(
                "Loaded shader {:?} ({:?}) {:?}",
                info.get_source_file(),
                info.get_shader_stage(),
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
            });
        }
        Ok(Self { shaders })
    }
}
