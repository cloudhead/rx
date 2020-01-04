uniform sampler2D cursor;
uniform sampler2D framebuffer;

in  vec2 f_uv;
out vec4 fragColor;

in  float f_scale;

void main() {
	vec2 fb_size = vec2(textureSize(framebuffer, 0));
	vec2 fb_coord = gl_FragCoord.xy / fb_size / f_scale;
	vec4 fb_texel = texture(
		framebuffer,
		// NOTE: This is inverted because the position in the
		// vertex shader is inverted.
		vec2(fb_coord.x, fb_size.y - fb_coord.y)
	);

	vec4 texel = texture(cursor, f_uv);

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
