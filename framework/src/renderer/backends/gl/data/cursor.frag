uniform sampler2D cursor;
uniform sampler2D framebuffer;
uniform bool      invert;

in  vec2 f_uv;
out vec4 fragColor;

void main() {
	vec2 fb_size = vec2(textureSize(framebuffer, 0));
	vec2 fb_coord = gl_FragCoord.xy / fb_size;
	vec4 fb_texel = texture(
		framebuffer,
		vec2(fb_coord.x, fb_coord.y)
	);

	vec4 texel = texture(cursor, f_uv);

	if (texel.a > 0.0) {
		if (invert && texel.rgb == vec3(1, 1, 1)) {
			fragColor = vec4(
				1.0 - fb_texel.r,
				1.0 - fb_texel.g,
				1.0 - fb_texel.b,
				1.0
			);
		} else {
			fragColor = texel;
		}
	} else {
		discard;
	}
}
