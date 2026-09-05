#version 150
precision highp float;

#define LIGHT vec3(0.36, 0.80, 0.48)

//Inputs
uniform sampler2DArray tex;
in vec3 ex_uv;
in vec3 ex_normal;
in vec3 ex_world;

// EXT: the flashlight's cone (src/ext/view.rs publishes these; ext/tool.rs aims it).
// GLSL 150 has no #include, so this block is duplicated by hand across the lit shaders the
// way LIGHT and EVE_SUN already are -- src/ext/view.rs owns a test that reads these files
// and fails if the copies drift.
uniform vec4 spot_pos;   // xyz world origin, w = range in metres (0 = off, and GL's default)
uniform vec4 spot_dir;   // xyz unit direction, w = cos(outer angle)
uniform vec4 spot_col;   // rgb radiance, w = cos(inner angle)

vec3 spot_light(vec3 P, vec3 N) {
	if (spot_pos.w <= 0.0) return vec3(0.0);
	vec3 d = spot_pos.xyz - P;
	float r = length(d);
	if (r >= spot_pos.w) return vec3(0.0);
	vec3 L = d / max(r, 1e-4);
	// The cone: full inside the core angle, faded to nothing by the outer one.
	float cone = smoothstep(spot_dir.w, spot_col.w, dot(-L, spot_dir.xyz));
	if (cone <= 0.0) return vec3(0.0);
	// Same falloff curve the door's light pool uses, so the two read as one lighting model.
	float atten = pow(max(1.0 - r / spot_pos.w, 0.0), 2.2);
	// Wrapped lambert: a torch beam grazing a wall should not have a hard terminator.
	float ndl = max(dot(N, L), 0.0) * 0.8 + 0.2;
	return spot_col.rgb * cone * atten * ndl;
}

//Outputs
out vec4 fragColor;

void main(void) {
	float s = dot(ex_normal, LIGHT)*0.25 + 0.75;
	vec3 col = texture(tex, ex_uv).rgb * s;
	vec3 sp = spot_light(ex_world, normalize(ex_normal));
	col += col * sp * 1.35 + sp * 0.055;
	fragColor = vec4(col, 1.0);
}
