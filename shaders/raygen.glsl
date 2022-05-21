#version 460
#pragma shader_stage(raygen)

#extension GL_NV_ray_tracing : require

layout(binding = 0, set = 0) uniform accelerationStructureNV topLevelAS;
layout(binding = 1, set = 0, rgba8) uniform image2D image;

layout( push_constant ) uniform constants
{
    vec4 light;
    mat4 view;
    mat4 model;
    mat4 proj;
} PushConstants;

layout(location = 0) rayPayloadNV vec3 hitValue;

void main() 
{
  const vec2 pixelCenter = vec2(gl_LaunchIDNV.xy) + vec2(0.5);
  const vec2 inUV = pixelCenter/vec2(gl_LaunchSizeNV.xy);
  vec2 d = inUV * 2.0 - 1.0;

  mat4 mvp = PushConstants.proj * PushConstants.view * PushConstants.model;
  mat4 viewInverse =inverse(mvp);
  vec4 origin = viewInverse * vec4(0,0,0,1);
  vec4 target = viewInverse * vec4(d.x, d.y, 1, 1);
  vec4 direction = viewInverse * vec4(normalize(target.xyz), 0);

  uint rayFlags = gl_RayFlagsOpaqueNV;
  uint cullMask = 0xff;
  float tmin = 0.001;
  float tmax = 10000.0;

  traceNV(topLevelAS, rayFlags, cullMask, 0 /*sbtRecordOffset*/, 0 /*sbtRecordStride*/, 0 /*missIndex*/, origin.xyz, tmin, direction.xyz, tmax, 0 /*payload*/);
  imageStore(image, ivec2(gl_LaunchIDNV.xy), vec4(hitValue, 0.0));
}
