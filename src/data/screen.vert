#version 450

layout(location = 0) in vec2 position;
layout(location = 1) in vec2 uv;

layout(location = 0) out vec2 f_uv;

void main() {
	f_uv = uv;
	gl_Position = vec4(position, 0.0, 1.0);
}
