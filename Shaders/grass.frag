#version 150
precision highp float;

// EXT: grass. No noise is computed here -- everything procedural was baked into
// Textures/grass_noise.bmp by tools/gen_meadow.py (R patchiness, G blade streaks, B mid
// variation). The look is a stack of cheap illusions on top of two or three texture taps:
//   1. CLOUD SHADOWS: a very low-frequency tap scrolled with time darkens the ground in slow
//      drifting patches. Nothing is cast or projected, but the eye reads "clouds overhead";
//   2. VALLEY OCCLUSION: low terrain is darker, ridges brighter -- world-space height as a
//      free ambient-occlusion term;
//   3. SUN SHEEN: backlit grass glows warm toward the sun, which is what makes the reference
//      read as "sunlit" rather than merely "green";
//   4. ATMOSPHERIC PERSPECTIVE: distance fades toward the sky's horizon colour, the single
//      most effective realism cue there is, for the price of one mix();
//   5. a slow luminance ripple that reads as wind without moving a vertex.

#define LIGHT vec3(0.36, 0.80, 0.48)

uniform sampler2D tex;
uniform vec4 cam_pos;
uniform float time;
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

// Weather grade (-1 daylight, 0 storm, 1 sunset) and the intro door's light pool (xyz, power).
uniform float mood;
uniform vec4 glow;
uniform float detail;   // 0 inside portal passes; see src/ext/view.rs

// Ground mist. Must match Shaders/grassblade.frag exactly -- see the note there.

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
in vec3 ex_world;
in vec3 ex_normal;
out vec4 fragColor;

void main(void) {
	vec3 n = normalize(ex_normal);
	vec3 L = normalize(LIGHT);
	vec2 uv = ex_world.xz;

	// Taps: broad patchiness, fine blades, and drifting cloud shadow (mipmapped, so all of
	// them stay calm at distance).
	// WIND. A gust field (a low-frequency tap scrolled with time) drives two things: the blade
	// pattern SWAYS (its texture coordinates shift with the gust, so the streaks visibly lean and
	// shimmer), and brightness WAVES sweep across the field -- the bands of light you see move
	// over a real meadow from a distance. One extra tap; no vertex moves.
	vec2 windDir = normalize(vec2(0.8, 0.55));
	float wind = 0.0;
	float flutter = 0.0;
	vec2 sway = vec2(0.0);
	if (detail > 0.5) {
		float gust = texture(tex, uv * snap_freq(0.018) + windDir * time * 0.09).b;
		float gust2 = texture(tex, uv * snap_freq(0.05) - windDir * time * 0.16).r;
		wind = (gust - 0.5) * 1.4 + (gust2 - 0.5) * 0.6;
		flutter = sin(time * 5.0 + uv.x * 1.7 + uv.y * 1.1 + gust * 9.0) * 0.5;
		sway = windDir * (wind * 0.9 + flutter * 0.25);
	}

	float patch  = texture(tex, uv * snap_freq(0.011)).r * 0.65 + texture(tex, uv * snap_freq(0.045)).b * 0.35;
	float blades = texture(tex, (uv + sway) * 0.30).g;
	float shade  = texture(tex, uv * snap_freq(0.0032) + vec2(time * 0.0035, time * 0.0012)).r;
	float cloudShadow = 1.0 - 0.42 * smoothstep(0.50, 0.78, shade);

	// Tussock-scale colour, matching Shaders/grassblade.frag: the same tap at the same
	// frequency, so the blade patch and the ground texture beyond its edge break up together
	// and the handover between them stays invisible.
	float tuft = texture(tex, uv * snap_freq(0.085)).b;
	vec3 lush = vec3(0.14, 0.34, 0.08);
	vec3 dry  = vec3(0.34, 0.40, 0.15);
	vec3 base = mix(lush, dry, smoothstep(0.28, 0.80, patch));
	base = mix(base, lush * 0.75, smoothstep(0.42, 0.05, tuft) * 0.45);
	base = mix(base, dry, smoothstep(0.68, 0.98, tuft) * 0.35);
	base *= 0.78 + 0.44 * blades;

	// Slope lighting: wrap diffuse plus a sharper sun-facing term so ridges catch light.
	float nl = dot(n, L);
	float diffuse = 0.28 + 0.55 * (nl * 0.5 + 0.5) + 0.35 * max(nl, 0.0);
	// Valley occlusion from world height (terrain spans roughly y in [0, 9]).
	float ao = mix(0.70, 1.0, smoothstep(0.0, 8.0, ex_world.y));
	vec3 col = base * diffuse * ao * cloudShadow;

	// Backlit sheen toward the sun.
	vec3 V = normalize(ex_world - cam_pos.xyz);
	float sheen = pow(max(dot(V, L), 0.0), 5.0);
	col += vec3(0.95, 0.85, 0.45) * sheen * 0.30 * (0.4 + 0.6 * blades) * cloudShadow;

	// ── Wildflowers ───────────────────────────────────────────────────────────────────────
	// The ground's half of the flowers in Shaders/grassblade.frag -- these are the ones showing
	// through the gaps between blades, and the ones still there past the blade patch's edge.
	// One candidate per cell of a lattice snapped to the world period, so the torus wrap cannot
	// slide the flowers sideways; faded out well before the cell is smaller than a pixel, where
	// they would boil into noise rather than resolve into dots.
	float fdist = length(ex_world - cam_pos.xyz);
	if (detail > 0.5 && fdist < 34.0) {
		float ff = snap_freq(2.2);
		vec2 g = uv * ff;
		vec2 cell = floor(g);
		float hh = fract(sin(dot(cell, vec2(127.1, 311.7))) * 43758.5453);
		if (hh > 0.86) {
			vec2 at = vec2(fract(hh * 731.0), fract(hh * 197.0));
			float bloom = smoothstep(0.26, 0.05, length(g - cell - at));
			bloom *= 1.0 - smoothstep(14.0, 34.0, fdist);
			float which = fract(hh * 53.0);
			vec3 petal = which > 0.62 ? vec3(0.95, 0.94, 0.88)
			           : (which > 0.26 ? vec3(0.98, 0.87, 0.36) : vec3(0.94, 0.71, 0.77));
			col = mix(col, petal * (diffuse * 0.55 + 0.45) * cloudShadow, bloom * 0.85);
		}
	}

	// Gust waves: leaning blades catch more light on the windward side.
	col *= 1.0 + 0.16 * wind + 0.05 * flutter;

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

	// Weather grade: the storm kills the sun and sinks the palette; sunset warms it.
	vec3 hazeCol = vec3(0.74, 0.82, 0.90);
	if (mood > 2.5) {
		// DUSK -- see Shaders/grassblade.frag, whose grade this matches.
		col = mix(col, vec3(dot(col, vec3(0.3, 0.59, 0.11))), 0.12) * vec3(0.54, 0.58, 0.62);
		// The sky's own dusk horizon (Shaders/sky.frag `hor`) scaled down, so distant ground
		// fades toward the colour of the sky it fades INTO. Aerial perspective is in-scattered
		// path radiance and converges on the sky's radiance in that direction; no scattering
		// process puts a green notch in it, and the previous R == B > G made the far knoll a
		// violet wedge.
		hazeCol = vec3(0.375, 0.385, 0.395);
	} else if (mood > 0.5) {
		col *= vec3(0.95, 0.72, 0.62);
		hazeCol = vec3(0.95, 0.62, 0.52);
	} else if (mood > -0.5) {
		col = mix(col, vec3(dot(col, vec3(0.3, 0.59, 0.11))), 0.18) * vec3(0.44, 0.54, 0.46);
		hazeCol = vec3(0.30, 0.34, 0.38);
	}


	// Atmospheric perspective toward the sky's horizon colour, plus the ground mist that
	// gathers in the hollows -- see Shaders/grassblade.frag, whose constants these are.
	float dist = length(ex_world - cam_pos.xyz);
	float fog = 1.0 - exp(-dist * 0.0105);
	float mist = exp(-max(ex_world.y - MIST_BASE, 0.0) * MIST_FALL) * (1.0 - exp(-dist * 0.028));
	// Capped short of 1: the far meadow keeps a trace of its own colour, so it stays a hair
	// DARKER than the sky it meets. Let it reach the haze colour exactly and the horizon
	// inverts -- the ground ends up brighter than the overcast above it and reads as a pale
	// band laid across the frame rather than as distance.
	hazeCol = mix(hazeCol, GLOW_WARM * 0.62, clamp(pool * 1.4, 0.0, 0.85));
	col = mix(col, hazeCol, clamp(fog * 0.9 + mist * MIST_DENSITY, 0.0, 0.93));

	fragColor = vec4(col, 1.0);
}
