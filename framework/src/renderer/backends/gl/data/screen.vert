const vec2[6] POSITION = vec2[](
	vec2(-1.0, -1.0),
	vec2( 1.0, -1.0),
	vec2( 1.0,  1.0),
	vec2(-1.0, -1.0),
	vec2(-1.0,  1.0),
	vec2( 1.0,  1.0)
);

const vec2[6] UV = vec2[](
	vec2(0.0, 0.0),
	vec2(1.0, 0.0),
	vec2(1.0, 1.0),
	vec2(0.0, 0.0),
	vec2(0.0, 1.0),
	vec2(1.0, 1.0)
);

out vec2 f_uv;

void main() {
	f_uv = UV[gl_VertexID];
	gl_Position = vec4(POSITION[gl_VertexID], 0.0, 1.0);
}
