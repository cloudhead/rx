uniform mat4 ortho;
uniform mat4 transform;

in vec3 position;
in float angle;
in vec2 center;
in vec4 color;

out vec4 f_color;

mat2 rotation2d(float angle) {
	float s = sin(angle);
	float c = cos(angle);
	return mat2(c, -s, s, c);
}

vec2 rotate(vec2 position, vec2 around, float angle) {
	mat2 m = rotation2d(angle);
	vec2 rotated = m * (position - around);
	return rotated + around;
}

// Convert an sRGB color to linear space.
vec3 linearize(vec3 srgb) {
	bvec3 cutoff = lessThan(srgb, vec3(0.04045));
	vec3 higher = pow((srgb + vec3(0.055)) / vec3(1.055), vec3(2.4));
	vec3 lower = srgb / vec3(12.92);

	return mix(higher, lower, cutoff);
}

void main() {
	vec2 r = rotate(position.xy, center, angle);

	f_color = vec4(linearize(color.rgb), color.a);
	gl_Position = ortho * transform * vec4(r, position.z, 1.0);
}
