#version 450

layout (location = 0) in vec3 vPosition;
layout (location = 1) in vec3 vNormal;
layout (location = 0) out vec3 outNormal;

layout( push_constant ) uniform constants
{
    vec4 light;
    mat4 view;
    mat4 model;
    mat4 proj;
} PushConstants;

void main()
{
    mat4 mvp = PushConstants.proj * PushConstants.view * PushConstants.model;
    gl_Position = mvp * vec4(vPosition, 1.0);
    outNormal = mat3(transpose(inverse(mvp))) * vNormal;
}
