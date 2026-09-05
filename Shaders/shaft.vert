#version 150

// EXT: one slice through the light beam coming out of the intro's door (src/ext/doorlight.rs).
// The slice is quad.obj, spanning [-1, 1]: `mvp` places and sizes it, `model` is the same
// placement without the camera, for the world position the fragment shader fogs and fades by.
uniform mat4 mvp;
uniform mat4 model;

in vec3 in_pos;
in vec2 in_uv;
in vec3 in_normal;

out vec2 ex_slice;    // position across the slice, [-1, 1]
out vec3 ex_world;
out vec3 ex_normal;   // the slice's own facing, for the grazing term

void main(void) {
	gl_Position = mvp * vec4(in_pos, 1.0);
	ex_slice = in_pos.xy;
	ex_world = (model * vec4(in_pos, 1.0)).xyz;
	// quad.obj lies in z = 0, so its normal is the model's third axis. Taken from the matrix
	// rather than assumed to be world +z: the slices carry the door's yaw, and a door that
	// faced any other way would otherwise get its grazing term from the wrong direction.
	ex_normal = normalize((model * vec4(0.0, 0.0, 1.0, 0.0)).xyz);
}
