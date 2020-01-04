uniform mat4 ortho;
uniform mat4 transform;

in vec3  position;
in vec2  uv;
in vec4  color;
in float opacity;

out vec2  f_uv;
out vec4  f_color;
out float f_opacity;

// Convert an sRGB color to linear space.
vec3 linearize(vec3 srgb) {
	bvec3 cutoff = lessThan(srgb, vec3(0.04045));
	vec3 higher = pow((srgb + vec3(0.055)) / vec3(1.055), vec3(2.4));
	vec3 lower = srgb / vec3(12.92);

	return mix(higher, lower, cutoff);
}

void main() {
	f_color = vec4(linearize(color.rgb), color.a);
	f_uv = uv;
	f_opacity = opacity;

	gl_Position = ortho * transform * vec4(position, 1.0);
}
