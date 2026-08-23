#version 150
// EXT: vertex stage for the rigid-body props (src/ext/rigid.rs). texture.vert plus the
// world-space position the fragment stage needs for its fog and its view vector. The three
// `in` attributes MUST stay in this order: the engine binds them by scanning for `in ` lines
// (Shader.cpp), and Mesh uploads position, uv, normal to locations 0, 1, 2.

//Globals
uniform mat4 mvp;
uniform mat4 mv;     // = transpose(world_to_local): the normal matrix, so it yields world space
uniform mat4 model;  // local_to_world, set by RigidProp::draw; draw_impl sets the two above

//Inputs
in vec3 in_pos;
in vec2 in_uv;
in vec3 in_normal;

//Outputs
out vec2 ex_uv;
out vec3 ex_normal;
out vec3 ex_world;

void main(void) {
	gl_Position = mvp * vec4(in_pos, 1.0);
	ex_uv = in_uv;
	ex_normal = normalize((mv * vec4(in_normal, 0.0)).xyz);
	ex_world = (model * vec4(in_pos, 1.0)).xyz;
}
