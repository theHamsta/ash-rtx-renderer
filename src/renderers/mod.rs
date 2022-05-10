pub mod color_sine;
pub mod ortho;

use std::rc::Rc;
use std::time::Instant;

use ash::vk::{self, SurfaceFormatKHR};
use enum_dispatch::enum_dispatch;
use winit::event::{DeviceEvent, WindowEvent};

use crate::device_mesh::DeviceMesh;
use crate::shader::ShaderPipeline;

use self::color_sine::ColorSine;
use self::ortho::Orthographic;

#[enum_dispatch]
pub trait Renderer<'device> {
    fn set_meshes(&mut self, _meshes: &[Rc<DeviceMesh<'device>>]) {}

    fn set_resolution(
        &mut self,
        _surface_format: SurfaceFormatKHR,
        _size: vk::Extent2D,
        _images: &[vk::Image],
        _device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
        _render_style: RenderStyle
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        start_instant: Instant,
        swapchain_idx: usize,
    ) -> anyhow::Result<()>;

    fn graphics_pipeline(&self) -> Option<&ShaderPipeline> {
        None
    }

    fn process_window_event(&mut self, _event: &WindowEvent) {}
    fn process_device_event(&mut self, _event: &DeviceEvent) {}
}

#[allow(clippy::large_enum_variant)]
#[enum_dispatch(Renderer)]
#[derive(Debug)]
pub enum RendererImpl<'device> {
    ColorSine(ColorSine),
    Orthographic(Orthographic<'device>),
}

#[derive(Debug, Copy, Eq, PartialEq, Clone)]
pub enum RenderStyle {
    Normal,
    Wireframe
}
