#version 450

layout(set = 2, binding = 0) uniform texture2D tex;
layout(set = 2, binding = 1) uniform sampler   sam;

layout(location = 0) in  vec2 f_uv;
layout(location = 0) out vec4 fragColor;

void main() {
	fragColor = texture(
		sampler2D(tex, sam),
		vec2(f_uv.s, f_uv.t)
	);
}
