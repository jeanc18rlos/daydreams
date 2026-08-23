#version 150

// EXT: sea plane for the intro level's far side. Same world-position trick as grass.vert.
uniform mat4 mvp;
uniform mat4 mv;

in vec3 in_pos;
in vec2 in_uv;
in vec3 in_normal;

out vec3 ex_world;

void main(void) {
	gl_Position = mvp * vec4(in_pos, 1.0);
	mat4 l2w = inverse(transpose(mv));
	ex_world = (l2w * vec4(in_pos, 1.0)).xyz;
}
