uniform mat4      ortho;
uniform mat4      transform;
uniform float     scale;
uniform sampler2D framebuffer;

in vec3  position;
in vec2  uv;

out vec2  f_uv;

void main() {
	f_uv = uv;

	vec2 fb_size = vec2(textureSize(framebuffer, 0));

	gl_Position = ortho * transform * vec4(
		position.x,
		position.y,
		position.z,
		1.0
	);
}
