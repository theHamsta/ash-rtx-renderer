use std::{rc::Rc, time::Instant};

use ash::vk;
use log::trace;

use crate::mesh::Mesh;

use super::Renderer;

#[derive(Debug)]
pub struct Orthographic {
    mesh: Option<Rc<Mesh>>,
}

impl Orthographic {
    pub fn new() -> Self {
        Orthographic { mesh: None }
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
        if let Some(mesh) = self.mesh {
            unsafe {}
        }
        Ok(())
    }

    fn set_mesh(&mut self, mesh: Rc<Mesh>) {
        self.mesh = Some(mesh);
    }
}
