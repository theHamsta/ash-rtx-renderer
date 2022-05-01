#version 450

layout (location = 0) in vec3 vPosition;

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
}
