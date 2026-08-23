#version 150
precision highp float;

// EXT: replaces the ported gradient+sun sky (kept as sky_plain.frag.txt). The clouds come
// from two panoramas baked by Shaders/cloudbake.* (see src/ext/skybake.rs), cross-faded so
// the cloud field evolves continuously; plus a slow longitude drift. Two texture fetches and
// the original sun term -- cheap enough for every portal pass.

#define LIGHT vec3(0.36, 0.80, 0.48)
#define SUN_SIZE 0.002
#define SUN_SHARPNESS 1.0
#define PI 3.14159265
// Revolutions per second of the drift. A cloud crosses the 60 degree view in ~25 s.
#define DRIFT 0.0065

uniform sampler2D tex;
uniform sampler2D tex2;
uniform float blend;
uniform float time;
// -1 daylight, 0 storm, 1 sunset (src/ext/view.rs). Colour grades of the same baked clouds.
uniform float mood;
in vec3 ex_normal;
out vec4 fragColor;

void main(void) {
	vec3 n = normalize(ex_normal);
	// Direction -> equirectangular (u wraps; v=0.5 at the horizon).
	float u = atan(n.x, -n.z) / (2.0 * PI) + 0.5 + time * DRIFT;
	float v = 0.5 + asin(clamp(n.y, -1.0, 1.0)) / PI;
	vec3 sky = mix(texture(tex, vec2(u, v)).rgb, texture(tex2, vec2(u, v)).rgb, blend);

	// The sun is drawn at runtime so it never drifts with the clouds (original formula).
	float s = dot(n, LIGHT) - 1.0 + SUN_SIZE;
	float sun = min(exp(s * SUN_SHARPNESS / SUN_SIZE), 1.0);

	if (mood > 0.5) {
		// SUNSET: warm horizon, violet zenith, clouds lit pink from below.
		float lum = dot(sky, vec3(0.3, 0.59, 0.11));
		float cloud = smoothstep(0.62, 0.95, lum);          // bright = cloud
		vec3 horizon = vec3(1.00, 0.58, 0.42);
		vec3 zenith  = vec3(0.30, 0.28, 0.52);
		float h = clamp(n.y * 1.6, 0.0, 1.0);
		vec3 base = mix(horizon, zenith, pow(h, 0.7));
		vec3 cloudCol = mix(vec3(0.55, 0.40, 0.55), vec3(1.0, 0.78, 0.68), smoothstep(0.62, 1.0, lum));
		sky = mix(base, cloudCol, cloud);
		sun = 0.0;                                          // the sun sits below the horizon
	} else if (mood > -0.5) {
		// STORM: desaturated, dark, heavier overhead.
		float lum = dot(sky, vec3(0.3, 0.59, 0.11));
		vec3 grey = vec3(lum);
		vec3 tint = vec3(0.36, 0.40, 0.46);
		sky = mix(sky, grey, 0.85) * tint * 1.1;
		sky *= mix(0.9, 0.55, clamp(n.y * 1.8, 0.0, 1.0));  // darker overhead
		sun = 0.0;
	}

	fragColor = vec4(max(sky, vec3(sun)), 1.0);
}
