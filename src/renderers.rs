pub mod color_sine;
pub mod raster;
pub mod ray_tracing;
pub mod cuda;

use std::rc::Rc;
use std::time::Instant;

use ash::vk::{self, SurfaceFormatKHR};
use enum_dispatch::enum_dispatch;
use winit::event::{DeviceEvent, WindowEvent};

use crate::device_mesh::DeviceMesh;
use crate::shader::ShaderPipeline;

use self::color_sine::ColorSine;
use self::cuda::Cuda;
use self::raster::Raster;
use self::ray_tracing::RayTrace;

#[enum_dispatch]
pub trait Renderer<'device> {
    fn set_meshes(
        &mut self,
        _meshes: &[Rc<DeviceMesh<'device>>],
        _cmd: vk::CommandBuffer,
        _graphics_queue: vk::Queue,
        _device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn set_resolution(
        &mut self,
        _surface_format: SurfaceFormatKHR,
        _size: vk::Extent2D,
        _images: &[vk::Image],
        _device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
        _render_style: RenderStyle,
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
    Raster(Raster<'device>),
    RayTrace(RayTrace<'device>),
    Cuda(Cuda),
}

#[derive(Debug, Copy, Eq, PartialEq, Clone)]
pub enum RenderStyle {
    Normal,
    Wireframe,
}
