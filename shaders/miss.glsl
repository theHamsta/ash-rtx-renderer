#version 460
#pragma shader_stage(miss)

#extension GL_EXT_ray_tracing : require

layout(location = 0) rayPayloadInEXT ivec3 hitValue;

void main()
{
  hitValue = ivec3(0, 0, 0);
}
