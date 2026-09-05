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

// Ground mist: the height it has thinned to nothing above, how fast it thins, and how much of
// the haze colour it is worth at its thickest. The meadow's terrain runs from y=0 in the
// hollows to y=9 on the door's knoll (ext/terrain.rs), so a base of 1.5 and a falloff of 0.30
// leave the crests clear and fill the valleys. Duplicated in Shaders/grass.frag, which has to
// agree with this exactly or the blade patch and the ground beyond it would sit in different
// weather.

// The door's light pool: how far it carries, and its colour. Nineteen units is most of the
// visible meadow, and deliberately so -- this is the ONLY light source in the shot, and a pool
// that stopped a few metres out read as a spotlight on a stage rather than as a door left open
// in a field. The 2.2 exponent is what keeps it from looking like a disc: it falls off quickly
// enough near the door to have a bright centre, and slowly enough far out to tint. Duplicated
// across the materials the meadow uses -- grass.frag, grassblade.frag, gltfpbr.frag -- which
// must agree, or the blades, the ground under them and the door itself would each be lit by a
// different lamp.
#define GLOW_REACH 19.0
#define GLOW_WARM vec3(1.00, 0.72, 0.42)

#define MIST_BASE 1.5
#define MIST_FALL 0.30
#define MIST_DENSITY 0.40

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
in float ex_h;

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
	// Three scales of variation, not one. The broad tap is the meadow's own geography --
	// tens of metres of it. The `tuft` tap is a metre or two, the same scale the vertex
	// shader clumps blade LENGTH at, so the patches that stand taller are also the patches
	// that are greener -- which is what makes them read as tussocks rather than as two
	// unrelated noise fields laid over each other. The per-blade scatter below is the third.
	float patch = texture(tex, ex_world.xz * snap_freq(0.013)).r;
	float tuft  = texture(tex, ex_world.xz * snap_freq(0.085)).b;
	vec3 deep   = vec3(0.10, 0.26, 0.06);
	vec3 mid    = vec3(0.19, 0.38, 0.09);
	vec3 dry    = vec3(0.34, 0.40, 0.14);   // muted, not straw
	vec3 base = mix(mid, dry, smoothstep(0.50, 0.92, patch));
	base = mix(base, deep, smoothstep(0.42, 0.05, tuft) * 0.45);
	base = mix(base, dry, smoothstep(0.68, 0.98, tuft) * 0.35);
	// Per-blade: some blades are darker/greener, a few are dry and pale.
	float v = ex_rand;
	base = mix(base, deep, smoothstep(0.55, 0.0, v) * 0.72);
	base = mix(base, dry,  smoothstep(0.82, 1.0, v) * 0.48);
	// And a little per-blade brightness on top of the hue: no two blades in a real field
	// catch the same amount of light, and an even one is the last thing that reads as CG.
	base *= 0.80 + 0.42 * fract(v * 17.0);

	// Cloud shadows: the same drifting low-frequency tap the ground material uses
	// (Shaders/grass.frag), at the same frequency and the same drift, so a patch of shade lies
	// across the blades and the texture beyond them as ONE shadow rather than two that
	// disagree along the seam. It is also the only thing that varies across the field at dusk
	// -- the sun is far enough away that every blade gets the same backlight -- and without it
	// the meadow reads as a flat sheet of gold.
	float shade = texture(tex, ex_world.xz * snap_freq(0.0032) + vec2(time * 0.0035, time * 0.0012)).r;
	float cloudShadow = 1.0 - 0.42 * smoothstep(0.50, 0.78, shade);

	// Occlusion in the canopy. Real grass is nearly black at the soil line -- light does not
	// reach down there. This vertical gradient is the single strongest cue that the field has
	// DEPTH rather than being a painted surface.
	//
	// Keyed to metres above the soil (`ex_h`), NOT to fraction-of-own-blade. Light in a
	// canopy attenuates with depth below the canopy top, Beer-Lambert, and depth is a
	// distance -- so a short blade's tip should be darker than a tall one's, and under the
	// old form every one of the 130,000 tips reached exactly 1.0 and the top of the sward was
	// a single flat tone. It is also what makes the clumping in the vertex shader read: a
	// short tussock now sits in shade as well as standing lower.
	float ao = 0.06 + 0.94 * (1.0 - exp(-ex_h * 4.0));

	float ndl = dot(n, L) * 0.5 + 0.5;   // wrap lighting, not a hard terminator
	// Fill light is the sky, so it takes the sky's colour: grey-blue by day and under the
	// storm, the violet of the dusk zenith in the evening (Shaders/sky.frag).
	vec3 sky = mix(vec3(0.42, 0.52, 0.62), vec3(0.30, 0.28, 0.46), step(2.5, mood));
	vec3 col = base * (ndl * 1.05 + 0.22) * ao * cloudShadow + base * sky * 0.14 * ao;

	// Translucency: thin blades glow when the sun is behind them. This is most of what makes
	// a lit field read as grass rather than green plastic.
	vec3 V = normalize(ex_world - cam_pos.xyz);
	float back = pow(max(dot(V, L), 0.0), 4.0);
	col += vec3(0.80, 0.92, 0.32) * back * 0.55 * ao * cloudShadow;

	// Fresnel rim along the silhouette.
	float fres = pow(clamp(1.0 + dot(n, V), 0.0, 1.0), 3.0);
	col += vec3(0.20, 0.22, 0.12) * fres * ao;

	// ── Wildflowers ───────────────────────────────────────────────────────────────────────
	// One blade in twenty-six carries a head at its very tip: a terminal blob a centimetre or
	// two across, on a stem that stays green beneath it. No extra vertices, no second draw and no new texture -- the flowers sway
	// with the blades because they ARE the blades -- and they are what stops a field of one
	// species reading as mown lawn. The selector is a different multiple of the same per-blade
	// random the sway uses, so flowers do not land in step with the wind phase.
	float pick = fract(ex_rand * 41.0);
	float shade_pick = fract(ex_rand * 173.0);
	// Three species rather than one. A meadow's flowers are not all the same colour, and it is
	// the SCATTER of a few whites, a few golds and the odd pink that reads as wild rather than
	// as a lawn someone has speckled.
	vec3 petal = shade_pick > 0.62 ? vec3(0.97, 0.96, 0.92)
	           : (shade_pick > 0.26 ? vec3(0.99, 0.88, 0.38) : vec3(0.95, 0.72, 0.78));
	float head = step(0.962, pick) * smoothstep(0.88, 0.955, ex_t);
	// Lit, not emissive: a petal in a cloud's shadow is still in shadow, and the weather grade
	// below reaches it like everything else -- under the overcast these are pale sparks in the
	// grass, and in the door's light they are the brightest thing in the meadow.
	col = mix(col, petal * (ndl * 0.55 + 0.55) * cloudShadow, head);

	// ── Seed heads ────────────────────────────────────────────────────────────────────────
	// A different seventeenth of the blades carry a russet head instead of a flower: the dry
	// inflorescences standing above the green that give any real meadow its broken, speckled
	// top edge. Darker than the grass rather than brighter, so they read against the sky the
	// way the flowers read against the ground.
	float seed = step(0.905, pick) * (1.0 - step(0.962, pick)) * smoothstep(0.80, 0.885, ex_t);
	col = mix(col, vec3(0.30, 0.21, 0.10) * (ndl * 0.7 + 0.5) * cloudShadow, seed * 0.85);

	// The open door spills warm light into the meadow. Two terms, because a doorway does two
	// things: it LIGHTS what it faces -- multiplied into the surface, so grass in shadow stays
	// grass and does not turn into yellow paint -- and it fills the air in front of it, which
	// is added regardless of what the surface is. The radius is wide on purpose: in the
	// reference this one light source tints most of the lower half of the frame, and a pool
	// that stopped a few metres out read as a spotlight on a stage.
	float pool = 0.0;
	if (glow.w > 0.0) {
		float gd = length(ex_world - glow.xyz);
		pool = glow.w * pow(max(1.0 - gd / GLOW_REACH, 0.0), 2.2);
		col += col * GLOW_WARM * pool * 1.35;
		col += GLOW_WARM * pool * 0.055;
	}

	// Weather grade, matching the ground material.
	vec3 haze = vec3(0.74, 0.82, 0.90);
	if (mood > 2.5) {
		// DUSK: the same cold overcast as the storm, an hour later -- so the grade is the
		// storm's, only the light it is under is violet instead of grey. Warmth is the door's
		// job, not the sky's.
		col = mix(col, vec3(dot(col, vec3(0.3, 0.59, 0.11))), 0.12) * vec3(0.56, 0.60, 0.64);
		// The sky's own dusk horizon (Shaders/sky.frag `hor`) scaled down, so distant ground
		// fades toward the colour of the sky it fades INTO. Aerial perspective is in-scattered
		// path radiance and converges on the sky's radiance in that direction; no scattering
		// process puts a green notch in it, and the previous R == B > G made the far knoll a
		// violet wedge.
		haze = vec3(0.375, 0.385, 0.395);
	} else if (mood > 0.5) {
		col *= vec3(0.98, 0.74, 0.62);
		haze = vec3(0.95, 0.62, 0.52);
	} else if (mood > -0.5) {
		col = mix(col, vec3(dot(col, vec3(0.3, 0.59, 0.11))), 0.18) * vec3(0.46, 0.56, 0.48);
		haze = vec3(0.30, 0.34, 0.38);
	}


	// Fade blades into the ground texture with distance -- this is the seam between the blade
	// patch and the textured ground, and it has to be invisible -- and then into the air.
	//
	// MIST. On top of the distance term, a second one that thickens toward low ground: the
	// hollows between the hills hold it and the crests stand out of it. It is what turns a
	// legible landscape into a half-remembered one, and it is nearly free -- the height it
	// keys on is the world position every line above already has.
	float dist = length(ex_world - cam_pos.xyz);
	float fog = 1.0 - exp(-dist * 0.0105);
	float mist = exp(-max(ex_world.y - MIST_BASE, 0.0) * MIST_FALL) * (1.0 - exp(-dist * 0.028));
	// Capped short of 1: the far meadow keeps a trace of its own colour, so it stays a hair
	// DARKER than the sky it meets. Let it reach the haze colour exactly and the horizon
	// inverts -- the ground ends up brighter than the overcast above it and reads as a pale
	// band laid across the frame rather than as distance.
	haze = mix(haze, GLOW_WARM * 0.62, clamp(pool * 1.4, 0.0, 0.85));
	col = mix(col, haze, clamp(fog * 0.9 + mist * MIST_DENSITY, 0.0, 0.93));

	fragColor = vec4(col, 1.0);
}
