#version 150

// EXT: the vertex half of every full-screen post pass (src/ext/postfx.rs). quad.obj already
// spans [-1, 1], so the position IS the clip position and the texture coordinate is it mapped
// into [0, 1]. `mvp`/`mv` are declared because `Shader::set_mvp` sets them for every draw; both
// are ignored here.
uniform mat4 mvp;
uniform mat4 mv;

in vec3 in_pos;
in vec2 in_uv;
in vec3 in_normal;

out vec2 ex_uv;

void main(void) {
	gl_Position = vec4(in_pos.xy, 0.0, 1.0);
	ex_uv = in_pos.xy * 0.5 + 0.5;
}
