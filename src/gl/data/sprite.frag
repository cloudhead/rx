uniform sampler2D tex;

in  vec2  f_uv;
in  vec4  f_color;
in  float f_opacity;

out vec4  fragColor;

void main() {
	vec4 texel = texture(tex, f_uv);

	fragColor = vec4(
		mix(texel.rgb, f_color.rgb, f_color.a),
		texel.a * f_opacity
	);
}
