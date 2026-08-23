#version 150
precision highp float;

// EXT: grass blade shading.
//
// The shading model follows MonterMan's "grass field with blades"
// (https://www.shadertoy.com/view/dd2cWh, CC BY-NC-SA 3.0) -- specifically its young/old grass
// colour mix, its height-from-terrain ambient occlusion `0.3 + 0.7*sqrt(h)`, and the fresnel
// rim iq suggested in that thread. The implementation here is original: this is a rasteriser
// drawing real blades, not a raymarched SDF, so none of the traversal code applies.

#define LIGHT vec3(0.36, 0.80, 0.48)

uniform sampler2D tex;    // grass_noise.bmp -- patchiness, reused from the ground material
uniform vec4 cam_pos;
uniform float time;
uniform float mood;       // -1 daylight, 0 storm, 1 sunset (src/ext/view.rs)
uniform vec4 glow;        // intro door light pool (xyz, strength)
uniform float wrap;       // world period on a wrapping scene, 0 otherwise (src/ext/view.rs)

// Round a world-space frequency to a whole number of cycles per world period.
//
// The intro meadow is a flat torus: the player's position is wrapped by exactly one period every
// time they cross the seam (ext/terrain.rs). That is invisible only if every pattern keyed to
// world position repeats over that same distance -- otherwise the wrap lands mid-cycle and the
// patchiness visibly jumps sideways. Off (wrap = 0) this is the identity, so ordinary scenes
// keep the frequencies they were tuned with.
float snap_freq(float f) {
	return wrap > 0.0 ? max(1.0, floor(f * wrap + 0.5)) / wrap : f;
}


in vec3 ex_world;
in vec3 ex_normal;
in float ex_t;
in float ex_rand;

out vec4 fragColor;

void main(void) {
	// One normal per blade, whichever side is seen. The blades are drawn with back-face
	// culling off (ext/grassfield.rs), so half of them face away from the camera -- and they
	// must shade exactly as they did when the patch carried both windings and culling picked
	// the front copy, which always had gl_FrontFacing set. Flipping on gl_FrontFacing here
	// would darken every blade seen from behind and change the look of the field.
	vec3 n = normalize(ex_normal);
	// Blades are thin and translucent: light wraps around them rather than terminating hard.
	vec3 L = normalize(LIGHT);

	// Colour: a broad patchiness field, PLUS strong per-blade variation. A lawn is not one
	// green -- individual blades run from yellow-dry to deep green, and it is that scatter
	// that stops a field reading as a solid mat.
	float patch = texture(tex, ex_world.xz * snap_freq(0.013)).r;
	vec3 deep   = vec3(0.10, 0.26, 0.06);
	vec3 mid    = vec3(0.19, 0.38, 0.09);
	vec3 dry    = vec3(0.34, 0.40, 0.14);   // muted, not straw
	vec3 base = mix(mid, dry, smoothstep(0.50, 0.92, patch));
	// Per-blade: some blades are darker/greener, a few are dry and pale.
	float v = ex_rand;
	base = mix(base, deep, smoothstep(0.55, 0.0, v) * 0.55);
	base = mix(base, dry,  smoothstep(0.88, 1.0, v) * 0.30);

	// Occlusion between blades. Real grass is nearly black at the soil line -- light does not
	// reach down there. This vertical gradient is the single strongest cue that the field has
	// DEPTH rather than being a painted surface.
	float ao = 0.10 + 0.90 * pow(clamp(ex_t, 0.0, 1.0), 0.75);

	float ndl = dot(n, L) * 0.5 + 0.5;   // wrap lighting, not a hard terminator
	vec3 sky = vec3(0.42, 0.52, 0.62);
	vec3 col = base * (ndl * 1.05 + 0.22) * ao + base * sky * 0.14 * ao;

	// Translucency: thin blades glow when the sun is behind them. This is most of what makes
	// a lit field read as grass rather than green plastic.
	vec3 V = normalize(ex_world - cam_pos.xyz);
	float back = pow(max(dot(V, L), 0.0), 4.0);
	col += vec3(0.80, 0.92, 0.32) * back * 0.55 * ao;

	// Fresnel rim along the silhouette.
	float fres = pow(clamp(1.0 + dot(n, V), 0.0, 1.0), 3.0);
	col += vec3(0.20, 0.22, 0.12) * fres * ao;

	// The open door spills warm light onto the ground in front of it. Applied BEFORE the
	// weather grade, so it reads as light in the scene rather than yellow paint on top of an
	// already-darkened surface -- which is what made the first version look radioactive.
	if (glow.w > 0.0) {
		float gd = length(ex_world - glow.xyz);
		float pool = glow.w * pow(max(1.0 - gd / 3.5, 0.0), 2.0);
		col += col * vec3(1.00, 0.86, 0.62) * pool * 0.9;
	}

	// Weather grade, matching the ground material.
	vec3 haze = vec3(0.74, 0.82, 0.90);
	if (mood > 0.5) {
		col *= vec3(0.98, 0.74, 0.62);
		haze = vec3(0.95, 0.62, 0.52);
	} else if (mood > -0.5) {
		col = mix(col, vec3(dot(col, vec3(0.3, 0.59, 0.11))), 0.18) * vec3(0.46, 0.56, 0.48);
		haze = vec3(0.30, 0.34, 0.38);
	}


	// Fade blades into the ground texture with distance -- this is the seam between the blade
	// patch and the textured ground, and it has to be invisible.
	float dist = length(ex_world - cam_pos.xyz);
	float fog = 1.0 - exp(-dist * 0.0080);
	col = mix(col, haze, fog * 0.9);

	fragColor = vec4(col, 1.0);
}
