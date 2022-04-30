use std::time::Instant;
use std::{os::raw::c_char, rc::Rc};

use anyhow::Context;
use ash::{
    extensions::khr,
    vk::{self, SurfaceFormatKHR},
};
use ash_swapchain::Swapchain;
use log::{error, info};
use winit::{dpi::PhysicalSize, window::Window};

#[derive(thiserror::Error, Debug)]
pub enum VulkanError {
    #[error("Found no device with surface support")]
    NoDeviceForSurfaceFound,
    #[error("Found no format for current surface")]
    NoSurfaceFormat,
}

struct Frame {
    cmd: vk::CommandBuffer,
    complete: vk::Semaphore,
}

struct Functions {
    surface: ash::extensions::khr::Surface,
    swapchain: ash::extensions::khr::Swapchain,
}

pub struct VulkanApp {
    instance: ash::Instance,
    surface: vk::SurfaceKHR,
    _entry: ash::Entry,
    physical_device: vk::PhysicalDevice,
    graphics_queue: vk::Queue,
    surface_format: vk::SurfaceFormatKHR,
    start_instant: Instant,
    device: Rc<ash::Device>,
    swapchain: Swapchain,
    frames: Vec<Frame>,
    functions: Functions,
    command_pool: vk::CommandPool,
    device_memory_properties: vk::PhysicalDeviceMemoryProperties,
}

impl VulkanApp {
    pub fn new(window: &Window) -> anyhow::Result<Self> {
        unsafe {
            let surface_extensions = ash_window::enumerate_required_extensions(window)?;
            let mut instance_extensions = surface_extensions.iter().copied().collect::<Vec<_>>();
            instance_extensions.push(khr::GetPhysicalDeviceProperties2::name().as_ptr());
            let app_desc = vk::ApplicationInfo::default()
                .api_version(vk::make_api_version(0, 1, 0, 0))
                .application_name(std::ffi::CStr::from_bytes_with_nul_unchecked(
                    b"ash-rtx-renderer",
                ));
            let instance_desc = vk::InstanceCreateInfo::default()
                .application_info(&app_desc)
                .enabled_extension_names(&instance_extensions);

            let entry = ash::Entry::load()?;
            let instance = entry.create_instance(&instance_desc, None)?;
            let surface = ash_window::create_surface(&entry, &instance, window, None)?;
            let surface_fn = khr::Surface::new(&entry, &instance);

            let (physical_device, queue_family_index) = instance
                .enumerate_physical_devices()
                .unwrap()
                .into_iter()
                .find_map(|dev| {
                    let mut props = vk::PhysicalDeviceProperties2KHR::default();
                    instance.get_physical_device_properties2(dev, &mut props);

                    info!(
                        "{:?}",
                        ::std::ffi::CStr::from_ptr(
                            props.properties.device_name.as_ptr() as *const c_char
                        )
                    );
                    let (family, _) = instance
                        .get_physical_device_queue_family_properties(dev)
                        .into_iter()
                        .enumerate()
                        .find(|(_index, info)| {
                            info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                        })?;
                    let family = family as u32;
                    let supported =
                        surface_fn.get_physical_device_surface_support(dev, family, surface);
                    match supported {
                        Ok(false) => return None,
                        Ok(true) => (),
                        Err(err) => {
                            error!(
                                "Failed to initialize surface for {:?}: {err}",
                                ::std::ffi::CStr::from_ptr(
                                    props.properties.device_name.as_ptr() as *const c_char
                                )
                            );
                            return None;
                        }
                    }

                    info!(
                        "Selected {:?}",
                        ::std::ffi::CStr::from_ptr(
                            props.properties.device_name.as_ptr() as *const c_char
                        )
                    );
                    Some((dev, family))
                })
                .ok_or(VulkanError::NoDeviceForSurfaceFound)?;

            let device = Rc::new(
                instance.create_device(
                    physical_device,
                    &vk::DeviceCreateInfo::default()
                        .enabled_extension_names(&[khr::Swapchain::name().as_ptr() as _])
                        .queue_create_infos(&[vk::DeviceQueueCreateInfo::default()
                            .queue_family_index(queue_family_index)
                            .queue_priorities(&[1.0])]),
                    None,
                )?,
            );
            let swapchain_fn = khr::Swapchain::new(&instance, &device);
            let graphics_queue = device.get_device_queue(queue_family_index, 0);
            let mut swapchain_options = ash_swapchain::Options::default();
            swapchain_options.frames_in_flight(4);
            let size = window.inner_size();
            let swapchain = Swapchain::new(
                &ash_swapchain::Functions {
                    device: &device,
                    swapchain: &swapchain_fn,
                    surface: &surface_fn,
                },
                swapchain_options,
                surface,
                physical_device,
                vk::Extent2D {
                    width: size.width,
                    height: size.height,
                },
            );

            let command_pool = device
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::default()
                        .flags(vk::CommandPoolCreateFlags::TRANSIENT)
                        .queue_family_index(queue_family_index),
                    None,
                )
                .unwrap();
            let cmds = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
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

            let surface_fn = ash::extensions::khr::Surface::new(&entry, &instance);

            let surface_format = *surface_fn
                .get_physical_device_surface_formats(physical_device, surface)?
                .get(0)
                .ok_or(VulkanError::NoSurfaceFormat)?;
            let device_memory_properties =
                instance.get_physical_device_memory_properties(physical_device);

            Ok(Self {
                _entry: entry,
                physical_device,
                surface_format,
                instance,
                surface,
                swapchain,
                frames,
                start_instant: Instant::now(),
                graphics_queue,
                device,
                command_pool,
                functions: Functions {
                    surface: surface_fn,
                    swapchain: swapchain_fn,
                },
                device_memory_properties,
            })
        }
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        self.swapchain.update(vk::Extent2D {
            width: size.width,
            height: size.height,
        });
    }

    pub fn draw(
        &mut self,
        draw_fn: impl Fn(
            &ash::Device,
            vk::CommandBuffer,
            vk::Image,
            Instant,
            usize,
        ) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let device = &self.device;
        unsafe {
            let acq = self
                .swapchain
                .acquire(
                    &ash_swapchain::Functions {
                        device: &self.device,
                        swapchain: &self.functions.swapchain,
                        surface: &self.functions.surface,
                    },
                    !0,
                )
                .context("Failed to acquire swapchain image")?;
            let cmd = self.frames[acq.frame_index].cmd;
            let swapchain_image = self.swapchain.images()[acq.image_index];
            device.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;

            draw_fn(
                &self.device,
                cmd,
                swapchain_image,
                self.start_instant,
                acq.frame_index,
            )?;

            device.end_command_buffer(cmd)?;
            device.queue_submit(
                self.graphics_queue,
                &[vk::SubmitInfo::default()
                    .wait_semaphores(&[acq.ready])
                    .wait_dst_stage_mask(&[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
                    .signal_semaphores(&[self.frames[acq.frame_index].complete])
                    .command_buffers(&[cmd])],
                acq.complete,
            )?;
            self.swapchain.queue_present(
                &ash_swapchain::Functions {
                    device: &self.device,
                    swapchain: &self.functions.swapchain,
                    surface: &self.functions.surface,
                },
                self.graphics_queue,
                self.frames[acq.frame_index].complete,
                acq.image_index,
            )?;
        }
        Ok(())
    }

    pub fn images(&self) -> &[vk::Image] {
        self.swapchain.images()
    }

    /// Get a reference to the vulkan app's device.
    #[must_use]
    pub fn device(&self) -> &Rc<ash::Device> {
        &self.device
    }

    /// Get the vulkan app's surface format.
    #[must_use]
    pub fn surface_format(&self) -> SurfaceFormatKHR {
        self.surface_format
    }

    pub(crate) fn device_memory_properties(&self) -> &vk::PhysicalDeviceMemoryProperties {
        &self.device_memory_properties
    }
}

impl Drop for VulkanApp {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            for frame in &self.frames {
                self.device.destroy_semaphore(frame.complete, None);
            }
            self.device.destroy_command_pool(self.command_pool, None);
            self.swapchain.destroy(&ash_swapchain::Functions {
                device: &self.device,
                swapchain: &self.functions.swapchain,
                surface: &self.functions.surface,
            });
            self.functions.surface.destroy_surface(self.surface, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}
