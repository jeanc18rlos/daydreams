#version 150

// EXT: grass material for the meadow terrain.
uniform mat4 mvp;
uniform mat4 mv;       // the NORMAL matrix = transpose(world_to_local) (Object.cpp:22)

in vec3 in_pos;
in vec3 in_uv;         // the SMOOTH NORMAL, smuggled through the 3-component vt channel
in vec3 in_normal;     // the engine's flat per-face normal (unused: facets)

out vec3 ex_world;
out vec3 ex_normal;

void main(void) {
	gl_Position = mvp * vec4(in_pos, 1.0);
	// mv = transpose(world_to_local)  =>  local_to_world = inverse(transpose(mv)).
	// Four vertices per hill means this inverse is effectively free.
	mat4 l2w = inverse(transpose(mv));
	ex_world = (l2w * vec4(in_pos, 1.0)).xyz;
	ex_normal = normalize((mv * vec4(in_uv, 0.0)).xyz);
}
