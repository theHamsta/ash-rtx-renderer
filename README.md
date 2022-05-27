TODOs

- [x] depth buffers
- [x] shading
- [~] panning
- [-] rtx rendering
- [-] meshlet rendering
- [-] ui?


fillModeNonSolid specifies whether point and wireframe fill modes are supported. If this feature is not enabled, the VK_POLYGON_MODE_POINT and VK_POLYGON_MODE_LINE enum values must not be used.

# Requirements

- You needs to have a nightly Rust toolchain installed: 
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
