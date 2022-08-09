use std::collections::HashSet;
use std::ffi::CString;
use std::time::Instant;
use std::{ffi::CStr, os::raw::c_char};

use anyhow::Context;
use ash::{
    extensions::khr,
    prelude::VkResult,
    vk::{self, SurfaceFormatKHR},
};
use ash_swapchain::Swapchain;
use log::{debug, error, info};
use tracing::{span, Level};
use tracy_client::frame_mark;
use winit::{dpi::PhysicalSize, window::Window};

#[derive(thiserror::Error, Debug)]
pub enum VulkanError {
    #[error("Found no device with surface support")]
    NoDeviceForSurfaceFound,
}

struct Frame {
    cmd: vk::CommandBuffer,
    complete: vk::Semaphore,
}

struct Functions {
    surface: ash::extensions::khr::Surface,
    swapchain: ash::extensions::khr::Swapchain,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TracingMode {
    NoTracing,
    Basic,
}

pub struct VulkanApp {
    instance: ash::Instance,
    surface: vk::SurfaceKHR,
    _entry: ash::Entry,
    graphics_queue: vk::Queue,
    start_instant: Instant,
    device: ash::Device,
    swapchain: Swapchain,
    frames: Vec<Frame>,
    functions: Functions,
    command_pool: vk::CommandPool,
    device_memory_properties: vk::PhysicalDeviceMemoryProperties,
    physical_device: vk::PhysicalDevice,
    tracing_mode: TracingMode,
    raytracing_support: bool,
}

impl VulkanApp {
    pub fn new(
        window: &Window,
        with_raytracing: bool,
        tracing_mode: TracingMode,
    ) -> anyhow::Result<Self> {
        unsafe {
            let surface_extensions = ash_window::enumerate_required_extensions(window)?;
            let instance_extensions = surface_extensions.to_vec();
            let app_desc = vk::ApplicationInfo::default()
                .api_version(vk::make_api_version(0, 1, 3, 204))
                .application_name(std::ffi::CStr::from_bytes_with_nul_unchecked(
                    b"ash-rtx-renderer\0",
                ));
            let instance_desc = vk::InstanceCreateInfo::default()
                .application_info(&app_desc)
                .enabled_extension_names(&instance_extensions);

            let entry = ash::Entry::load()?;
            let instance = entry.create_instance(&instance_desc, None)?;
            let surface = ash_window::create_surface(&entry, &instance, window, None)?;
            let surface_fn = khr::Surface::new(&entry, &instance);

            let mut supported_devices: Vec<_> = instance
                .enumerate_physical_devices()
                .context("Failed to enumerate physical devices")?
                .into_iter()
                .filter_map(|dev| {
                    let mut props = vk::PhysicalDeviceProperties2KHR::default();
                    instance.get_physical_device_properties2(dev, &mut props);

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
                        Ok(false) => {
                            info!(
                                "Not eligible for current surface {:?}",
                                ::std::ffi::CStr::from_ptr(
                                    props.properties.device_name.as_ptr() as *const c_char
                                )
                            );
                            return None;
                        }
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
                        "Supported {:?}",
                        ::std::ffi::CStr::from_ptr(
                            props.properties.device_name.as_ptr() as *const c_char
                        )
                    );
                    Some((dev, family, props))
                })
                .collect();
            let first_nvidia_device =
                supported_devices
                    .iter()
                    .copied()
                    .find_map(|(physical_device, index, props)| {
                        if props.properties.vendor_id == 0x10DE {
                            Some((physical_device, index, props))
                        } else {
                            None
                        }
                    });
            let (physical_device, queue_family_index, props) = first_nvidia_device
                .or_else(|| supported_devices.pop())
                .ok_or(VulkanError::NoDeviceForSurfaceFound)?;
            info!(
                "Selected {:?}",
                ::std::ffi::CStr::from_ptr(props.properties.device_name.as_ptr() as *const c_char)
            );

            let mut extensions = HashSet::new();
            for ext in instance.enumerate_device_extension_properties(physical_device)? {
                extensions.insert(CStr::from_ptr(ext.extension_name.as_ptr()).to_owned());
                debug!("Device supports: {ext:?}");
            }
            let mut features11 = vk::PhysicalDeviceVulkan11Features::default();
            let mut features12 = vk::PhysicalDeviceVulkan12Features::default()
                .buffer_device_address(true)
                .vulkan_memory_model(true);

            let mut as_feature = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default()
                .acceleration_structure(true);

            let mut raytracing_pipeline =
                vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::default()
                    .ray_tracing_pipeline(true);

            let mut enabled_extension_names = vec![khr::Swapchain::name().as_ptr()];

            let raytracing_support = with_raytracing
                && add_if_supported(
                    &extensions,
                    &[
                        ash::extensions::khr::RayTracingPipeline::name(),
                        ash::extensions::khr::AccelerationStructure::name(),
                        ash::extensions::khr::DeferredHostOperations::name(),
                    ],
                    &mut enabled_extension_names,
                );
            info!("Raytracing support: {raytracing_support}");

            let queue_create_info = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family_index)
                .queue_priorities(&[1.0])];

            let device_create_info = if raytracing_support {
                vk::DeviceCreateInfo::default()
                    .enabled_extension_names(&enabled_extension_names)
                    .push_next(&mut features11)
                    .push_next(&mut features12)
                    .push_next(&mut as_feature)
                    .push_next(&mut raytracing_pipeline)
                    .queue_create_infos(&queue_create_info)
            } else {
                vk::DeviceCreateInfo::default()
                    .enabled_extension_names(&enabled_extension_names)
                    .queue_create_infos(&queue_create_info)
            };
            let mut physical_device_features = vk::PhysicalDeviceFeatures2::default();
            physical_device_features.push_next(&mut vk::PhysicalDeviceVulkan11Features::default());
            physical_device_features.push_next(&mut vk::PhysicalDeviceVulkan12Features::default());
            device_create_info.push_next(&mut physical_device_features);
            let device = instance.create_device(physical_device, &device_create_info, None)?;
            let swapchain_fn = khr::Swapchain::new(&instance, &device);
            let graphics_queue = device.get_device_queue(queue_family_index, 0);
            let mut swapchain_options = ash_swapchain::Options::default();
            swapchain_options
                .frames_in_flight(3)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::STORAGE);
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

            let device_memory_properties =
                instance.get_physical_device_memory_properties(physical_device);

            let mut rt_pipeline_properties =
                vk::PhysicalDeviceRayTracingPipelinePropertiesKHR::default();

            if raytracing_support {
                let mut physical_device_properties2 =
                    vk::PhysicalDeviceProperties2::default().push_next(&mut rt_pipeline_properties);

                instance.get_physical_device_properties2(
                    physical_device,
                    &mut physical_device_properties2,
                );
            }

            Ok(Self {
                _entry: entry,
                instance,
                surface,
                swapchain,
                frames,
                start_instant: Instant::now(),
                graphics_queue,
                device,
                command_pool,
                physical_device,
                functions: Functions {
                    surface: surface_fn,
                    swapchain: swapchain_fn,
                },
                device_memory_properties,
                tracing_mode,
                raytracing_support,
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
        if self.tracing_mode == TracingMode::Basic {
            frame_mark();
        }
        let span = span!(Level::INFO, "draw");
        let _ = span.enter();
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

            let span = span!(Level::INFO, "drawing");
            let _ = span.enter();
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
    pub fn device(&self) -> &ash::Device {
        &self.device
    }

    /// Get the vulkan app's surface format.
    #[must_use]
    pub fn surface_format(&self) -> SurfaceFormatKHR {
        self.swapchain.format()
    }

    pub(crate) fn device_memory_properties(&self) -> &vk::PhysicalDeviceMemoryProperties {
        &self.device_memory_properties
    }

    /// Get a reference to the vulkan app's instance.
    #[must_use]
    pub fn instance(&self) -> &ash::Instance {
        &self.instance
    }

    pub fn graphics_queue(&self) -> vk::Queue {
        self.graphics_queue
    }

    pub fn allocate_command_buffers(&self, count: u32) -> VkResult<Vec<vk::CommandBuffer>> {
        unsafe {
            self.device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(self.command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(count),
            )
        }
    }
    pub fn free_command_buffers(&self, cmd: &[vk::CommandBuffer]) {
        unsafe {
            self.device.free_command_buffers(self.command_pool, cmd);
        }
    }

    pub fn rt_pipeline_properties(
        physical_device: vk::PhysicalDevice,
        instance: ash::Instance,
    ) -> vk::PhysicalDeviceRayTracingPipelinePropertiesKHR<'static> {
        let mut rt_pipeline_properties =
            vk::PhysicalDeviceRayTracingPipelinePropertiesKHR::default();

        let mut physical_device_properties2 =
            vk::PhysicalDeviceProperties2::default().push_next(&mut rt_pipeline_properties);

        unsafe {
            instance
                .get_physical_device_properties2(physical_device, &mut physical_device_properties2);
        }
        rt_pipeline_properties
    }

    pub fn physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    pub fn raytracing_support(&self) -> bool {
        self.raytracing_support
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

fn add_if_supported(
    supported: &HashSet<CString>,
    ext: &[&'static CStr],
    vec: &mut Vec<*const c_char>,
) -> bool {
    if ext.iter().all(|ext| supported.contains(ext.to_owned())) {
        for ext in ext.iter() {
            vec.push(ext.as_ptr());
        }
        true
    } else {
        false
    }
}
