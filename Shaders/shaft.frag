#version 150
precision highp float;

// EXT: one slice through the light beam (src/ext/doorlight.rs). Sixteen of these, stepped out
// along the sun's direction and ADDED, are the volume.
//
// A slice is not a lit surface, so there is no lighting here. What it carries is how much haze
// the light crossed at this point of the beam's cross-section, which is three things
// multiplied together:
//
//   * the SHAPE of the hole the light came through -- a rectangle, feathered at its edges,
//     because a hard-edged slice would show the stack as sixteen nested rectangles;
//   * the DUST in the air, as slow noise, so the beam is streaked and alive rather than a
//     clean wedge of colour. Two scales, drifting at different speeds, which is enough to
//     stop the pattern reading as a texture;
//   * the ANGLE it is seen at. A slice seen edge-on covers a sliver of screen and should
//     contribute almost nothing, but a flat quad's coverage falls off far faster than the
//     depth of air behind it does, so without this the beam would vanish as the camera swung
//     round to the side of it.

uniform float gain;    // the slice's own weight: brightness, falloff and the door's openness
uniform float time;
uniform vec4 cam_pos;

// The colour of the light coming through. Warmer than the sunset's own horizon (Shaders/
// sky.frag) because what reaches here has crossed a few metres of dusty air on the way.
#define BEAM_COL vec3(1.00, 0.70, 0.38)

in vec2 ex_slice;
in vec3 ex_world;
in vec3 ex_normal;
out vec4 fragColor;

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}
float vnoise(vec2 p) {
	vec2 i = floor(p), f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash(i), hash(i + vec2(1, 0)), f.x),
	           mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), f.x), f.y);
}

void main(void) {
	// Feathered rectangle: full weight through the middle of the opening, nothing past its
	// edge. Squared, so the two axes' falloffs meet in a rounded corner rather than a mitre.
	vec2 e = 1.0 - smoothstep(0.30, 1.0, abs(ex_slice));
	float shape = e.x * e.y;
	shape *= shape;

	// Dust, in world space so it does not swim across the beam as the slices move.
	float d1 = vnoise(ex_world.xz * 1.7 + vec2(time * 0.05, -time * 0.03));
	float d2 = vnoise(ex_world.xy * 3.4 - vec2(time * 0.09, time * 0.04));
	float dust = 0.55 + 0.75 * (d1 * 0.65 + d2 * 0.35);

	// Grazing angles: a slice seen edge-on has almost no screen area but the same air behind
	// it, so give back what the projection took. Clamped, or the term explodes exactly at 90
	// degrees and the beam flashes as the camera crosses its plane.
	vec3 V = normalize(cam_pos.xyz - ex_world);
	float facing = abs(dot(V, normalize(ex_normal)));
	// Boosted, but bounded twice over: capped, and faded right out below a few degrees. A
	// slice seen truly edge-on covers a LINE of pixels, and multiplying a line by a large
	// number is how a volumetric effect turns into a scratch across the frame.
	float graze = clamp(1.0 / max(0.40, facing), 0.0, 1.9) * smoothstep(0.02, 0.14, facing);

	float a = gain * shape * dust * graze;
	fragColor = vec4(BEAM_COL, a);
}
