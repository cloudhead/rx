#version 450

layout(set = 0, binding = 0) uniform Global {
	mat4 ortho;
} global;

layout(set = 1, binding = 0) uniform Local {
	mat4 transform;
} local;

layout(location = 0) in vec4 position;
layout(location = 1) in vec2 uv;

layout(location = 0) out vec2 f_uv;

void main() {
	f_uv = uv;

	gl_Position = global.ortho * local.transform * position;
}
