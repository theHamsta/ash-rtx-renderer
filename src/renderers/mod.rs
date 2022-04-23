mod color_sine;
mod ortho;

use std::rc::Rc;
use std::time::Instant;

use ash::vk;
use enum_dispatch::enum_dispatch;

use crate::mesh::Mesh;

use self::color_sine::ColorSine;
use self::ortho::Orthographic;

#[enum_dispatch]
pub trait Renderer {
    fn set_mesh(&mut self, mesh: Rc<Mesh>);
    fn draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        start_instant: Instant,
    ) -> anyhow::Result<()>;
}

#[enum_dispatch(Drawer)]
#[derive(Debug)]
pub enum RendererImpl {
    ColorSine(ColorSine),
    Orthographic(Orthographic),
}
