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

out vec4 fragColor;

void main(void) {
	vec3 base = texture(tex, ex_uv).rgb;
	vec3 n = normalize(ex_normal);
	vec3 col;
	if (mood > 1.5) {
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
	fragColor = vec4(col, 1.0);
}
