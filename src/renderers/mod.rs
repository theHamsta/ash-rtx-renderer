pub mod color_sine;
pub mod ortho;

use std::rc::Rc;
use std::time::Instant;

use ash::vk::{self, SurfaceFormatKHR};
use enum_dispatch::enum_dispatch;

use crate::mesh::Mesh;

use self::color_sine::ColorSine;
use self::ortho::Orthographic;

#[enum_dispatch]
pub trait Renderer {
    fn set_mesh(&mut self, mesh: Rc<Mesh>) {}
    fn set_resolution(
        &mut self,
        device: &ash::Device,
        surface_format: SurfaceFormatKHR,
        size: vk::Extent2D,
        images: &[vk::Image],
    ) {
    }
    fn draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        start_instant: Instant,
    ) -> anyhow::Result<()>;
}

#[enum_dispatch(Renderer)]
#[derive(Debug)]
pub enum RendererImpl {
    ColorSine(ColorSine),
    Orthographic(Orthographic),
}
