#version 460
#pragma shader_stage(closest)

#extension GL_EXT_ray_tracing : require
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_buffer_reference2 : enable

layout(location = 0) rayPayloadInEXT vec3 hitValue;

hitAttributeEXT vec3 attribs;
layout(binding = 0, set = 0) uniform accelerationStructureEXT topLevelAS;

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

void main()
{
  const vec3 barycentrics = vec3(1.0 - attribs.x - attribs.y, attribs.x, attribs.y);

  uvec3 index = indices.i[gl_PrimitiveID];
  vec3 n0 = normals.n[3 * index.x + 0];
  vec3 n1 = normals.n[3 * index.y + 1];
  vec3 n2 = normals.n[3 * index.z + 2];
  vec3 normal = normalize(n0 * barycentrics.x + n1 * barycentrics.y + n2 * barycentrics.z);
  hitValue = vec3(1.0, index.xy / 100.0);
  
  //prd.hitT                = gl_HitTEXT;
  //prd.primitiveID         = gl_PrimitiveID;
  //prd.instanceID          = gl_InstanceID;
  //prd.instanceCustomIndex = gl_InstanceCustomIndexEXT;
  //prd.baryCoord           = bary;
  //prd.objectToWorld       = gl_ObjectToWorldEXT;
  //prd.worldToObject       = gl_WorldToObjectEXT;

    //uint64_t end  = clockRealtimeEXT();
}
