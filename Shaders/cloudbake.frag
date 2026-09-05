#version 150
precision highp float;

// EXT: one-off cloud panorama bake (see src/ext/skybake.rs). Runs at 1536x768 once every few
// seconds, so it can afford real FBM; the runtime sky shader only samples the result.
//
// Equirectangular layout: u = longitude, v = latitude with v=0.5 at the horizon, v=1 at the
// zenith. The runtime shader maps a view direction to the same (u,v).
//
// Technique: the classic "2D clouds on a dome" illusion. View directions above the horizon are
// projected onto a flat cloud layer, and a domain-warped FBM sampled there gives cumulus
// shapes. Lighting is a single scatter approximation -- compare the density here with the
// density one step toward the sun; where the cloud thins toward the sun its edge is lit.
//
// RGB is that daylight sky. ALPHA is the coverage on its own -- how much cloud is in front of
// the gradient at this pixel, before any of it is coloured. The runtime sky shader's evening
// grades want the SHAPE of the cloud field and none of its colour (src/ext/skybake.rs, "Why
// the panorama has an alpha channel"), and this is the only place that knows it exactly.

#define LIGHT vec3(0.36, 0.80, 0.48)
#define PI 3.14159265

uniform float time;
in vec2 ex_uv;
out vec4 fragColor;

// Integer hash -> value noise. Deterministic, so re-bakes only move with `time`.
float hash(vec2 p) {
	uvec2 q = uvec2(ivec2(floor(p))) * uvec2(1597334677u, 3812015801u);
	uint n = (q.x ^ q.y) * 1597334677u;
	return float(n) * (1.0 / 4294967295.0);
}
float vnoise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	float a = hash(i), b = hash(i + vec2(1, 0)), c = hash(i + vec2(0, 1)), d = hash(i + vec2(1, 1));
	return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}
float fbm(vec2 p) {
	float v = 0.0, a = 0.5;
	mat2 r = mat2(0.8, 0.6, -0.6, 0.8);   // rotate per octave: hides lattice alignment
	for (int i = 0; i < 6; ++i) {
		v += a * vnoise(p);
		p = r * p * 2.05 + vec2(1.7, 9.2);
		a *= 0.5;
	}
	return v;
}

void main(void) {
	float lon = (ex_uv.x - 0.5) * 2.0 * PI;
	float lat = (ex_uv.y - 0.5) * PI;
	vec3 dir = vec3(cos(lat) * sin(lon), sin(lat), -cos(lat) * cos(lon));
	vec3 sun = normalize(LIGHT);

	// ── Sky gradient + haze ───────────────────────────────────────────────────────────
	float y = max(dir.y, 0.0);
	vec3 zenith  = vec3(0.22, 0.47, 0.86);
	vec3 horizon = vec3(0.70, 0.80, 0.91);
	vec3 haze    = vec3(0.80, 0.86, 0.92);
	vec3 col = mix(horizon, zenith, pow(y, 0.55));
	col = mix(col, haze, pow(1.0 - y, 9.0));
	// Warm the sky around the sun a touch (cheap aerial scattering).
	float s = max(dot(dir, sun), 0.0);
	col += vec3(0.30, 0.22, 0.10) * pow(s, 6.0) * 0.35;

	if (dir.y <= 0.0) {
		// Below the horizon: ground haze. Terrain covers this; the edge of the world blends.
		// No cloud down here, hence the zero alpha.
		fragColor = vec4(mix(haze, vec3(0.62, 0.70, 0.62), min(-dir.y * 4.0, 1.0)), 0.0);
		return;
	}

	// ── Cumulus layer ─────────────────────────────────────────────────────────────────
	float h = 1.0 / (dir.y + 0.10);             // flat layer projection, softened at horizon
	// 1.45, not 1.9: a lower frequency means BIGGER clouds for the same field. The evening
	// grade in Shaders/sky.frag paints each cloud as a lit shape rather than fading it into
	// haze, so a field of small ones reads as a scatter of wisps where the sunset wants
	// masses.
	vec2 p = dir.xz * h * 1.45 + vec2(time * 0.35, time * 0.12);
	vec2 q = vec2(fbm(p * 0.6), fbm(p * 0.6 + vec2(5.2, 1.3)));
	vec2 w = p + q * 1.1;                        // domain warp: lumpy, cauliflower edges
	float d = fbm(w);

	// WEATHER. A very low frequency field that decides how much cloud there is HERE, before
	// any of the shapes are cut out of it. Without it the coverage threshold is the same
	// everywhere and the sky comes out evenly speckled -- the giveaway of every procedural
	// cloud layer. With it there are banks and there are clearings, and the cumulus only grow
	// where the weather allows, which is the order the real thing happens in.
	float weather = fbm(p * 0.11 + vec2(3.7, 8.1));
	float floor_ = mix(0.60, 0.36, smoothstep(0.35, 0.75, weather));
	float cov = smoothstep(floor_, floor_ + 0.20, d);
	cov = pow(cov, 0.8);

	// Lighting: a three-step march toward the sun rather than one. Each step asks how much
	// cloud is still between this point and the light, and the answers accumulate -- so a
	// deep flank stays dark while a thin edge lights up along its whole length, which one
	// sample cannot tell apart. Three is where the flanks stop looking like a bevel.
	vec2 toSun = normalize(sun.xz) * 0.055;
	float shade_sum = 0.0;
	for (int k = 1; k <= 3; ++k) {
		shade_sum += max(fbm(w + toSun * float(k)) - d, 0.0) / float(k);
	}
	float lit = clamp(1.0 - shade_sum * 4.2, 0.0, 1.0);
	lit = mix(lit, 1.0, 0.18);   // never fully black: clouds scatter light around themselves
	// Tops lit, bottoms shadowed: the density itself approximates depth.
	float thick = smoothstep(0.55, 0.9, d);
	vec3 shadow = vec3(0.58, 0.63, 0.74);
	vec3 bright = vec3(1.02, 1.00, 0.97);
	vec3 cloud = mix(bright, shadow, thick * 0.75) ;
	cloud = mix(cloud, bright, lit * 0.6);
	// Silver lining toward the sun.
	cloud += vec3(0.25, 0.20, 0.12) * pow(s, 3.0) * lit * 0.5;

	// Fade the layer into the haze at the horizon (also hides projection stretching).
	cov *= smoothstep(0.0, 0.22, dir.y);

	// ── Thin high cirrus ──────────────────────────────────────────────────────────────
	// Stretched hard along one axis, which is what makes it read as fibre rather than as more
	// cumulus, and drifting the other way from the layer below -- two decks moving against
	// each other is most of what gives a sky depth.
	vec2 pc = dir.xz * h * 0.8 + vec2(-time * 0.2, time * 0.05);
	float ci = fbm(vec2(pc.x * 0.7, pc.y * 3.0));
	float cirrus = smoothstep(0.55, 0.75, ci) * 0.35 * smoothstep(0.05, 0.4, dir.y);
	// A second deck, higher and finer, at right angles to the first.
	vec2 pc2 = dir.xz * h * 0.5 + vec2(time * 0.11, -time * 0.24);
	float ci2 = fbm(vec2(pc2.x * 2.6, pc2.y * 0.6));
	cirrus = max(cirrus, smoothstep(0.60, 0.80, ci2) * 0.22 * smoothstep(0.10, 0.5, dir.y));

	col = mix(col, vec3(0.96, 0.97, 1.0), cirrus);
	col = mix(col, cloud, cov);

	// Coverage, for the evening grades. The cirrus counts for part of its own opacity: it is
	// thin, and a sunset paints it as a wisp rather than as a cumulus flank.
	fragColor = vec4(col, clamp(cov + cirrus * 0.55, 0.0, 1.0));
}
