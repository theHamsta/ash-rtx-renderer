TODOs

- [x] depth buffers
- [x] shading
- [~] panning
- [-] rtx rendering
- [-] meshlet rendering
- [-] ui?


fillModeNonSolid specifies whether point and wireframe fill modes are supported. If this feature is not enabled, the VK_POLYGON_MODE_POINT and VK_POLYGON_MODE_LINE enum values must not be used.


TODO: destroy objects needed for raytracing
    Objects: 1
        [0] 0x55ca46b93380, type: 1, name: NULL
VUID-vkDestroyDevice-device-00378(ERROR / SPEC): msgNum: 1901072314 - Validation Error: [ VUID-vkDestroyDevice-device-00378 ] Object 0: handle = 0x55ca475aa370, type = VK_OBJECT_TYPE_DEVICE; Object 1: handle = 0x9638f80000000036, type = VK_OBJECT_TYPE_DESCRIPTOR_SET_LAYOUT; | MessageID = 0x71500fba | OBJ ERROR : For VkDevice 0x55ca475aa370[], VkDescriptorSetLayout 0x9638f80000000036[] has not been destroyed. The Vulkan spec states: All child objects created on device must have been destroyed prior to destroying device (https://vulkan.lunarg.com/doc/view/1.3.211.0/linux/1.3-extensions/vkspec.html#VUID-vkDestroyDevice-device-00378)
    Objects: 2
        [0] 0x55ca475aa370, type: 3, name: NULL
        [1] 0x9638f80000000036, type: 20, name: NULL
VUID-vkDestroyDevice-device-00378(ERROR / SPEC): msgNum: 1901072314 - Validation Error: [ VUID-vkDestroyDevice-device-00378 ] Object 0: handle = 0x55ca475aa370, type = VK_OBJECT_TYPE_DEVICE; Object 1: handle = 0x808562000000003f, type = VK_OBJECT_TYPE_DESCRIPTOR_SET; | MessageID = 0x71500fba | OBJ ERROR : For VkDevice 0x55ca475aa370[], VkDescriptorSet 0x808562000000003f[] has not been destroyed. The Vulkan spec states: All child objects created on device must have been destroyed prior to destroying device (https://vulkan.lunarg.com/doc/view/1.3.211.0/linux/1.3-extensions/vkspec.html#VUID-vkDestroyDevice-device-00378)
    Objects: 2
        [0] 0x55ca475aa370, type: 3, name: NULL
        [1] 0x808562000000003f, type: 23, name: NULL
VUID-vkDestroyDevice-device-00378(ERROR / SPEC): msgNum: 1901072314 - Validation Error: [ VUID-vkDestroyDevice-device-00378 ] Object 0: handle = 0x55ca475aa370, type = VK_OBJECT_TYPE_DEVICE; Object 1: handle = 0x5c5283000000003e, type = VK_OBJECT_TYPE_DESCRIPTOR_POOL; | MessageID = 0x71500fba | OBJ ERROR : For VkDevice 0x55ca475aa370[], VkDescriptorPool 0x5c5283000000003e[] has not been destroyed. The Vulkan spec states: All child objects created on device must have been destroyed prior to destroying device (https://vulkan.lunarg.com/doc/view/1.3.211.0/linux/1.3-extensions/vkspec.html#VUID-vkDestroyDevice-device-00378)
    Objects: 2
        [0] 0x55ca475aa370, type: 3, name: NULL
        [1] 0x5c5283000000003e, type: 22, name: NULL
VUID-vkDestroyDevice-device-00378(ERROR / SPEC): msgNum: 1901072314 - Validation Error: [ VUID-vkDestroyDevice-device-00378 ] Object 0: handle = 0x55ca475aa370, type = VK_OBJECT_TYPE_DEVICE; Object 1: handle = 0xa7c5450000000023, type = VK_OBJECT_TYPE_ACCELERATION_STRUCTURE_KHR; | MessageID = 0x71500fba | OBJ ERROR : For VkDevice 0x55ca475aa370[], VkAccelerationStructureKHR 0xa7c5450000000023[] has not been destroyed. The Vulkan spec states: All child objects created on device must have been destroyed prior to destroying device (https://vulkan.lunarg.com/doc/view/1.3.211.0/linux/1.3-extensions/vkspec.html#VUID-vkDestroyDevice-device-00378)
    Objects: 2
        [0] 0x55ca475aa370, type: 3, name: NULL
        [1] 0xa7c5450000000023, type: 1000150000, name: NULL
VUID-vkDestroyDevice-device-00378(ERROR / SPEC): msgNum: 1901072314 - Validation Error: [ VUID-vkDestroyDevice-device-00378 ] Object 0: handle = 0x55ca475aa370, type = VK_OBJECT_TYPE_DEVICE; Object 1: handle = 0xcb1c7c000000001b, type = VK_OBJECT_TYPE_ACCELERATION_STRUCTURE_KHR; | MessageID = 0x71500fba | OBJ ERROR : For VkDevice 0x55ca475aa370[], VkAccelerationStructureKHR 0xcb1c7c000000001b[] has not been destroyed. The Vulkan spec states: All child objects created on device must have been destroyed prior to destroying device (https://vulkan.lunarg.com/doc/view/1.3.211.0/linux/1.3-extensions/vkspec.html#VUID-vkDestroyDevice-device-00378)
    Objects: 2
        [0] 0x55ca475aa370, type: 3, name: NULL
        [1] 0xcb1c7c000000001b, type: 1000150000, name: NULL

# Requirements

- You need to have a nightly Rust toolchain installed: 
```
curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain nightly
```
- The build.rs script will try to use `glslc` (https://github.com/google/shaderc part of the Vulkan SDK https://www.lunarg.com/vulkan-sdk/) and `nvcc` to compile the shaders.

# Run the sample 

You can run the sample using
```
cargo run -- --mesh-file <path-to-obj-or-ply-mesh>
```
Note that the mesh must contain only triangles (not quads).
