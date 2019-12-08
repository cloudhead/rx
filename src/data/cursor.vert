#version 450

layout(set = 0, binding = 0) uniform Globals {
	mat4  ortho;
	float scale;
} global;

layout(location = 0) in vec3  position;
layout(location = 1) in vec2  uv;

layout(location = 0) out vec2  f_uv;
layout(location = 1) out float f_scale;

void main() {
	f_uv = uv;
	f_scale = global.scale;

	gl_Position = global.ortho * vec4(position, 1.0);
}
