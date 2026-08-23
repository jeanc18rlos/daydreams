#version 150

// EXT: silhouette mask pass. Plain transform, flat output.
uniform mat4 mvp;
uniform mat4 mv;

in vec3 in_pos;
in vec2 in_uv;
in vec3 in_normal;

void main(void) {
	gl_Position = mvp * vec4(in_pos, 1.0);
}
