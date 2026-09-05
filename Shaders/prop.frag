#version 150
precision highp float;

// EXT: material for the rigid-body props (src/ext/rigid.rs): a textured thing that may sit in
// a lit interior or under the ported sun.
//
// INTERIOR (mood > 1.5, src/ext/view.rs): the same hemisphere gltfpbr.frag lights the
// elevator and the overgrown room with -- warm white from above, a dim brown bounce from the
// floor, a small ambient floor -- and its damped overhead highlight, so an apple on the
// Backrooms' carpet is lit by the same lamps as the cabin beside it rather than by a sun
// that is not there. Then the walls' own squared-distance fog toward `fog_color`
// (gltfunlit.frag), which the caller sets to the building's tone; the initialiser is the
// Backrooms' WALL_FOG for a caller that forgets, so a far prop fades into the hall and not
// into black.
//
// Anywhere else: the ported wrap light, texture.frag:15 exactly, so a prop in a ported room
// is lit as the room is.

#define LIGHT vec3(0.36, 0.80, 0.48)
#define HEMI_UP vec3(0.90, 0.864, 0.792)
#define HEMI_DOWN vec3(0.25, 0.22, 0.18)
#define AMBIENT vec3(0.06)

uniform sampler2D tex;
uniform vec4 cam_pos;   // this pass's eye, world space
uniform float mood;     // -1 daylight, 0 storm, 1 sunset, 2 interior
uniform vec4 fog_color = vec4(0.40, 0.33, 0.16, 1.0);  // = backrooms::WALL_FOG

in vec2 ex_uv;
in vec3 ex_normal;
in vec3 ex_world;

// EXT: the flashlight's cone (src/ext/view.rs publishes these; ext/tool.rs aims it).
// GLSL 150 has no #include, so this block is duplicated by hand across the lit shaders the
// way LIGHT and EVE_SUN already are -- src/ext/view.rs owns a test that reads these files
// and fails if the copies drift.
uniform vec4 spot_pos;   // xyz world origin, w = range in metres (0 = off, and GL's default)
uniform vec4 spot_dir;   // xyz unit direction, w = cos(outer angle)
uniform vec4 spot_col;   // rgb radiance, w = cos(inner angle)

uniform vec4 shine;      // rgb = colour, a = strength; zero = ordinary scenery

// EXT: what an interactive object does when the beam finds it. The rim term is why it reads
// as "that one is a THING" rather than "that one is brighter": a silhouette lights before a
// face does, which is how an eye picks an object out of a cluttered room.
vec3 shine_answer(vec3 P, vec3 N, vec3 V, vec3 lit) {
	if (shine.a <= 0.0) return vec3(0.0);
	float rim = pow(1.0 - max(dot(N, V), 0.0), 3.0);
	return shine.rgb * shine.a * lit * (0.35 + 1.30 * rim);
}

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

out vec4 fragColor;

void main(void) {
	vec3 base = texture(tex, ex_uv).rgb;
	vec3 n = normalize(ex_normal);
	vec3 col;
	if (mood > 1.5 && mood < 2.5) {   // interior; 3 is dusk and must not land here
		vec3 hemi = mix(HEMI_DOWN, HEMI_UP, n.y * 0.5 + 0.5) + AMBIENT;
		col = base * hemi;
		// A soft highlight from overhead, as gltfpbr.frag gives a dielectric of middling
		// roughness: enough for a polished apple or a lacquered piece to catch the lamps.
		vec3 V = normalize(cam_pos.xyz - ex_world);
		vec3 H = normalize(vec3(0.0, 1.0, 0.0) + V);
		float spec = pow(max(dot(n, H), 0.0), 48.0);
		col += spec * 0.18;
		float d = length(ex_world - cam_pos.xyz) * 0.0195;
		float fog = 1.0 - exp(-d * d);
		col = mix(col, fog_color.rgb, fog);
	} else {
		float s = dot(n, LIGHT) * 0.5 + 0.5;
		col = base * s;
	}
	// The torch, on both branches: a prop the beam finds should light up wherever it stands.
	vec3 sp = spot_light(ex_world, n);
	col += col * sp * 1.35 + sp * 0.055;
	col += shine_answer(ex_world, n, normalize(cam_pos.xyz - ex_world), sp);
	fragColor = vec4(col, 1.0);
}
