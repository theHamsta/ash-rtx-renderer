use ash::vk;
use winit::window::Window;

pub struct VulkanApp {
    entry: ash::Entry,
    instance: ash::Instance,
    surface: vk::SurfaceKHR,
}

impl VulkanApp {
    pub fn new(window: &Window) -> anyhow::Result<Self> {
        unsafe {
            let surface_extensions = ash_window::enumerate_required_extensions(window)?;
            let instance_extensions = surface_extensions
                .iter()
                .map(|ext| ext.as_ptr())
                .collect::<Vec<_>>();
            let app_desc =
                vk::ApplicationInfo::builder().api_version(vk::make_api_version(0, 1, 0, 0));
            let instance_desc = vk::InstanceCreateInfo::builder()
                .application_info(&app_desc)
                .enabled_extension_names(&instance_extensions);

            let entry = ash::Entry::linked();
            let instance = entry.create_instance(&instance_desc, None)?;
            let surface = ash_window::create_surface(&entry, &instance, window, None)?;

            Ok(Self {
                entry,
                instance,
                surface,
            })
        }
    }
}

impl Drop for VulkanApp {
    fn drop(&mut self) {
        let surface_fn = ash::extensions::khr::Surface::new(&self.entry, &self.instance);
        unsafe {
            surface_fn.destroy_surface(self.surface, None);
        }
    }
}
