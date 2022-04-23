use ash::vk;
use enum_dispatch::enum_dispatch;
use log::trace;

#[enum_dispatch]
pub trait Drawer {
    fn draw(&self, device: &ash::Device, cmd: &vk::CommandBuffer, image: &vk::Image);
}

#[enum_dispatch(Drawer)]
#[derive(Debug)]
pub enum DrawImpl {
    Triangle(TriangleDrawer),
}

#[derive(Debug)]
pub struct TriangleDrawer {}

impl TriangleDrawer {
    pub fn new() -> Self {
        TriangleDrawer {}
    }
}

impl Drawer for TriangleDrawer {
    fn draw(&self, device: &ash::Device, cmd: &vk::CommandBuffer, image: &vk::Image) {
        trace!("draw for {self:?}");
    }
}
