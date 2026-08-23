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

	vec3 lush = vec3(0.14, 0.34, 0.08);
	vec3 dry  = vec3(0.34, 0.40, 0.15);
	vec3 base = mix(lush, dry, smoothstep(0.28, 0.80, patch));
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

	// Gust waves: leaning blades catch more light on the windward side.
	col *= 1.0 + 0.16 * wind + 0.05 * flutter;

	// The open door spills warm light onto the ground in front of it. Applied BEFORE the
	// weather grade, so it reads as light in the scene rather than yellow paint on top of an
	// already-darkened surface -- which is what made the first version look radioactive.
	if (glow.w > 0.0) {
		float gd = length(ex_world - glow.xyz);
		float pool = glow.w * pow(max(1.0 - gd / 3.5, 0.0), 2.0);
		col += col * vec3(1.00, 0.86, 0.62) * pool * 0.9;
	}

	// Weather grade: the storm kills the sun and sinks the palette; sunset warms it.
	vec3 hazeCol = vec3(0.74, 0.82, 0.90);
	if (mood > 0.5) {
		col *= vec3(0.95, 0.72, 0.62);
		hazeCol = vec3(0.95, 0.62, 0.52);
	} else if (mood > -0.5) {
		col = mix(col, vec3(dot(col, vec3(0.3, 0.59, 0.11))), 0.18) * vec3(0.44, 0.54, 0.46);
		hazeCol = vec3(0.30, 0.34, 0.38);
	}


	// Atmospheric perspective toward the sky's horizon colour.
	float dist = length(ex_world - cam_pos.xyz);
	float fog = 1.0 - exp(-dist * 0.0080);
	col = mix(col, hazeCol, fog * 0.9);

	fragColor = vec4(col, 1.0);
}
