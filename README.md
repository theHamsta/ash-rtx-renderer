TODOs

- [x] depth buffers
- [x] shading
- [~] panning
- [x] rtx rendering
- [-] meshlet rendering
- [-] ui?


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
