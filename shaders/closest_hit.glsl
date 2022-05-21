#version 460
#pragma shader_stage(closest)

#extension GL_EXT_ray_tracing : require
#extension GL_EXT_nonuniform_qualifier : enable

layout(location = 0) rayPayloadInEXT vec3 hitValue;

hitAttributeEXT vec3 attribs;
layout(binding = 0, set = 0) uniform accelerationStructureEXT topLevelAS;

layout(shaderRecordEXT) buffer Indices { uint i[]; } indices;
layout(shaderRecordEXT) buffer Normals { vec3 v[]; } normals;

void main()
{
  const vec3 barycentrics = vec3(1.0 - attribs.x - attribs.y, attribs.x, attribs.y);

  ivec3 index = ivec3(indices.i[3 * gl_PrimitiveID], indices.i[3 * gl_PrimitiveID + 1], indices.i[3 * gl_PrimitiveID + 2]);
  vec3 n0 = normals.v[3 * index.x + 0];
  vec3 n1 = normals.v[3 * index.y + 1];
  vec3 n2 = normals.v[3 * index.z + 2];
  vec3 normal = normalize(n0 * barycentrics.x + n1 * barycentrics.y + n2 * barycentrics.z);
  hitValue = vec3(1.0, normal.xy);
  
  //prd.hitT                = gl_HitTEXT;
  //prd.primitiveID         = gl_PrimitiveID;
  //prd.instanceID          = gl_InstanceID;
  //prd.instanceCustomIndex = gl_InstanceCustomIndexEXT;
  //prd.baryCoord           = bary;
  //prd.objectToWorld       = gl_ObjectToWorldEXT;
  //prd.worldToObject       = gl_WorldToObjectEXT;

    //uint64_t end  = clockRealtimeEXT();
}
