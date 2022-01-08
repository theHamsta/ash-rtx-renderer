use ash::{extensions::khr, vk};
use ash_swapchain::Swapchain;
use winit::window::Window;

struct Frame {
    cmd: vk::CommandBuffer,
    complete: vk::Semaphore,
}

pub struct VulkanApp {
    entry: ash::Entry,
    instance: ash::Instance,
    surface: vk::SurfaceKHR,
    swapchain: Swapchain,
    frames: Vec<Frame>,
    render_queue: vk::Queue,
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

            let entry = ash::Entry::new();
            let instance = entry.create_instance(&instance_desc, None)?;
            let surface = ash_window::create_surface(&entry, &instance, window, None)?;
            let surface_fn = khr::Surface::new(&entry, &instance);

            let (physical_device, queue_family_index) = instance
                .enumerate_physical_devices()
                .unwrap()
                .into_iter()
                .find_map(|dev| {
                    let (family, _) = instance
                        .get_physical_device_queue_family_properties(dev)
                        .into_iter()
                        .enumerate()
                        .find(|(_index, info)| {
                            info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                        })?;
                    let family = family as u32;
                    let supported = surface_fn
                        .get_physical_device_surface_support(dev, family, surface)
                        .unwrap();
                    if !supported {
                        return None;
                    }
                    Some((dev, family))
                })
                .unwrap();

            let device = instance
                .create_device(
                    physical_device,
                    &vk::DeviceCreateInfo::builder()
                        .enabled_extension_names(&[khr::Swapchain::name().as_ptr() as _])
                        .queue_create_infos(&[vk::DeviceQueueCreateInfo::builder()
                            .queue_family_index(queue_family_index)
                            .queue_priorities(&[1.0])
                            .build()]),
                    None,
                )
                .unwrap();
            let swapchain_fn = khr::Swapchain::new(&instance, &device);
            let render_queue = device.get_device_queue(queue_family_index, queue_family_index);

            let size = window.inner_size();
            let swapchain = Swapchain::new(
                &ash_swapchain::Functions {
                    device: &device,
                    swapchain: &swapchain_fn,
                    surface: &surface_fn,
                },
                ash_swapchain::Options::default(),
                surface,
                physical_device,
                vk::Extent2D {
                    width: size.width,
                    height: size.height,
                },
            );

            let command_pool = device
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::builder()
                        .flags(
                            vk::CommandPoolCreateFlags::TRANSIENT
                                | vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
                        )
                        .queue_family_index(queue_family_index),
                    None,
                )
                .unwrap();
            let cmds = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::builder()
                        .command_pool(command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(swapchain.frames_in_flight() as u32),
                )
                .unwrap();
            let frames = cmds
                .into_iter()
                .map(|cmd| Frame {
                    cmd,
                    complete: device
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                        .unwrap(),
                })
                .collect();

            Ok(Self {
                entry,
                instance,
                surface,
                swapchain,
                frames,
                render_queue,
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
