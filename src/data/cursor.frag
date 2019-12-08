#version 450

layout(set = 1, binding = 0) uniform texture2D cursor;
layout(set = 1, binding = 1) uniform sampler   sampler0;

layout(set = 2, binding = 0) uniform texture2D framebuffer;

layout(location = 0) in  vec2 f_uv;
layout(location = 0) out vec4 fragColor;

layout(location = 1) in  float f_scale;

void main() {
	vec2 fb_size = vec2(textureSize(sampler2D(framebuffer, sampler0), 0));
	vec2 fb_coord = gl_FragCoord.xy / fb_size / f_scale;
	vec4 fb_texel = texture(sampler2D(framebuffer, sampler0), fb_coord);

	vec4 texel = texture(sampler2D(cursor, sampler0), vec2(f_uv.s, f_uv.t));

	if (texel.a > 0.0) {
		fragColor = vec4(
			1.0 - fb_texel.r,
			1.0 - fb_texel.g,
			1.0 - fb_texel.b,
			1.0
		);
	} else {
		discard;
	}
}
