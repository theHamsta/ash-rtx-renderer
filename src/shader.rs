use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
};

use ash::{util::read_spv, vk};
use log::debug;
use rspirv_reflect::Reflection;
use spirv_reflect::types::ReflectInterfaceVariable;

use crate::mesh::Mesh;

pub struct Shader {
    module: vk::ShaderModule,
    info: spirv_reflect::ShaderModule,
    alt_info: Reflection,
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
            let alt_info = Reflection::new_from_spirv(bytes)
                .map_err(|err| anyhow::anyhow!("Failed to get reflection info: {err}"))?;
            debug!(
                "Loaded shader {:?} ({:?}) in: {:?}, out: {:?} _push_constant_blocks {:?}",
                info.get_source_file(),
                info.get_shader_stage(),
                info.enumerate_input_variables(None),
                info.enumerate_output_variables(None),
                info.enumerate_push_constant_blocks(None)
            );

            debug!(
                "Info {:?} \n {:?}",
                alt_info.get_descriptor_sets(),
                alt_info.get_descriptor_sets()
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
                alt_info,
            });
        }
        Ok(Self { shaders })
    }

    pub fn allocate_for_mesh(&self, mesh: &Mesh) -> anyhow::Result<HashMap<String, vk::Buffer>> {
        let mut outputs = HashSet::new();
        for shader in self.shaders.iter() {
            for var in shader
                .info
                .enumerate_output_variables(None)
                .map_err(|err| {
                    anyhow::anyhow!(
                        "Failed to enumerate shader output variables: {}",
                        err.to_owned()
                    )
                })?
            {
                outputs.insert(var.name);
            }
        }

        let mut buffers = HashMap::new();
        for shader in self.shaders.iter() {
            for var in shader.info.enumerate_input_variables(None).map_err(|err| {
                anyhow::anyhow!(
                    "Failed to enumerate shader input variables: {}",
                    err.to_owned()
                )
            })? {
                if !outputs.contains(&var.name) {
                    let ReflectInterfaceVariable {
                        name,
                        location,
                        type_description,
                        word_offset,
                        format,
                        ..
                    } = var;
                }
            }
        }

        Ok(buffers)
    }
}
