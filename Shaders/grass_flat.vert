#version 150

// EXT: grass material for flat ground props (ground.obj / ground_slope.obj), which have no
// smooth-normal channel; the flat per-face normal is exact for them.
uniform mat4 mvp;
uniform mat4 mv;       // the NORMAL matrix = transpose(world_to_local) (Object.cpp:22)

in vec3 in_pos;
in vec2 in_uv;         // ordinary 2D texcoords (unused: grass maps from world position)
in vec3 in_normal;     // the engine's flat per-face normal -- exact for a flat quad

out vec3 ex_world;
out vec3 ex_normal;

void main(void) {
	gl_Position = mvp * vec4(in_pos, 1.0);
	// mv = transpose(world_to_local)  =>  local_to_world = inverse(transpose(mv)).
	// Four vertices per hill means this inverse is effectively free.
	mat4 l2w = inverse(transpose(mv));
	ex_world = (l2w * vec4(in_pos, 1.0)).xyz;
	ex_normal = normalize((mv * vec4(in_normal, 0.0)).xyz);
}
