#version 150

// EXT: fullscreen quad. quad.obj already spans [-1,1]; pass it through untransformed and
// derive the screen UV from position so no atlas flip questions arise.
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
