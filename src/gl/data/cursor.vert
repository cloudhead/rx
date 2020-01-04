uniform mat4      ortho;
uniform float     scale;
uniform sampler2D framebuffer;

in vec3  position;
in vec2  uv;

out vec2  f_uv;
out float f_scale;

void main() {
	f_uv = uv;
	f_scale = scale;

	vec2 fb_size = vec2(textureSize(framebuffer, 0));

	gl_Position = ortho * vec4(
		position.x,
		// NOTE: This is inverted because something is not right
		// in the pipeline. Something is flipped that shouldn't
		// be!
		fb_size.y - position.y,
		position.z,
		1.0
	);
}
