#version 150

//Globals
uniform mat4 mvp;
uniform mat4 mv;
// EXT: the object transform, for the world position a spotlight needs.
uniform mat4 model;

//Inputs
in vec3 in_pos;
in vec3 in_uv;
in vec3 in_normal;

//Outputs
out vec3 ex_uv;
out vec3 ex_normal;
out vec3 ex_world;

void main(void) {
	gl_Position = mvp * vec4(in_pos, 1.0);
	ex_uv = in_uv;
	ex_world = (model * vec4(in_pos, 1.0)).xyz;
	ex_normal = normalize((mv * vec4(in_normal, 0.0)).xyz);
}
