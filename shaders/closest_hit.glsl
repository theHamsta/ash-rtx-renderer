#version 460
#pragma shader_stage(closest)

#extension GL_EXT_ray_tracing : require
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_buffer_reference2 : enable

layout(location = 0) rayPayloadInEXT vec3 hitValue;
layout(location = 1) rayPayloadEXT vec3 next;
layout(binding = 0, set = 0) uniform accelerationStructureEXT topLevelAS;

hitAttributeEXT vec3 attribs;
//layout(binding = 0, set = 0) uniform accelerationStructureEXT topLevelAS;

layout(buffer_reference, buffer_reference_align = 8, scalar)
buffer NormalBuffer {
  vec3 n[];
};
layout(buffer_reference, buffer_reference_align = 8, scalar)
buffer IndexBuffer {
  uvec3 i[];
};

layout(shaderRecordEXT, std430) buffer SBT {
  IndexBuffer indices;
  NormalBuffer normals;
};

layout( push_constant ) uniform constants
{
    vec4 light;
    mat4 view;
    mat4 model;
    mat4 proj;
} PushConstants;

void main()
{
  const vec3 barycentrics = vec3(1.0 - attribs.x - attribs.y, attribs.x, attribs.y);


  uvec3 index = indices.i[gl_PrimitiveID];
  vec3 n0 = normals.n[index.x];
  vec3 n1 = normals.n[index.y];
  vec3 n2 = normals.n[index.z];
  vec3 normal = normalize(n0 * barycentrics.x + n1 * barycentrics.y + n2 * barycentrics.z);

  vec3 hitPos = gl_WorldRayOriginEXT + gl_HitTEXT * gl_WorldRayDirectionEXT;

  //mat4 mvp = PushConstants.proj * PushConstants.model;
  //normal = mat3(transpose(inverse(mvp))) * normal;
  normal = normalize(vec3(normal * gl_WorldToObjectEXT));

  uint rayFlags = gl_RayFlagsOpaqueEXT;
  uint cullMask = 0xff;
  float tmin = 1;
  float tmax = 100.0;

  vec3 direction = gl_WorldRayDirectionEXT - 2 * dot(gl_WorldRayDirectionEXT, normal);
  traceRayEXT(topLevelAS, rayFlags, cullMask, 0 /*sbtRecordOffset*/, 0 /*sbtRecordStride*/, 0 /*missIndex*/, hitPos, tmin, direction, tmax, 1 /*payload*/);
  //hitValue = 0.1 * normal + next;
  //if (barycentrics.x < 0.06 || barycentrics.y < 0.06 || barycentrics.z < 0.06) {
  hitValue = normal + 0.5;
  //} else  {
  //hitValue = vec3(0,0,0);
  //}
  
  //prd.hitT                = gl_HitTEXT;
  //prd.primitiveID         = gl_PrimitiveID;
  //prd.instanceID          = gl_InstanceID;
  //prd.instanceCustomIndex = gl_InstanceCustomIndexEXT;
  //prd.baryCoord           = bary;
  //prd.objectToWorld       = gl_ObjectToWorldEXT;
  //prd.worldToObject       = gl_WorldToObjectEXT;

    //uint64_t end  = clockRealtimeEXT();
}
