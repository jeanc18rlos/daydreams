#version 150
precision highp float;

// EXT: a cheap sunset sea. Two scrolled noise taps fake the wave field, a Fresnel term blends
// the water colour toward the sky's horizon colour at grazing angles, and a long specular
// streak toward the (below-horizon) sun gives the reference's glittering band. No reflection
// pass -- the "reflected sky" is just the same gradient the sky shader uses, evaluated here.

uniform sampler2D tex;       // grass_noise.bmp, reused purely as a noise source
uniform vec4 cam_pos;
uniform float time;
uniform float mood;
in vec3 ex_world;
out vec4 fragColor;

void main(void) {
	vec2 uv = ex_world.xz;
	// Wave field: two octaves drifting in different directions.
	float w1 = texture(tex, uv * 0.045 + vec2(time * 0.020, time * 0.013)).g;
	float w2 = texture(tex, uv * 0.120 - vec2(time * 0.031, time * 0.009)).b;
	float wave = (w1 - 0.5) * 0.8 + (w2 - 0.5) * 0.5;

	// A normal tilted by the wave slope (cheap finite difference via the two taps).
	vec3 n = normalize(vec3(wave * 0.9, 1.0, (w2 - w1) * 0.6));

	vec3 V = normalize(cam_pos.xyz - ex_world);
	float fresnel = pow(1.0 - max(dot(n, V), 0.0), 3.0);

	vec3 deep    = vec3(0.10, 0.22, 0.34);
	vec3 shallow = vec3(0.22, 0.42, 0.52);
	vec3 horizon = (mood > 0.5) ? vec3(1.00, 0.58, 0.42) : vec3(0.74, 0.82, 0.90);
	vec3 water = mix(deep, shallow, clamp(wave + 0.5, 0.0, 1.0));
	vec3 col = mix(water, horizon, fresnel * 0.85);

	// Glitter streak toward the sun's azimuth (the sun itself is below the horizon at sunset).
	vec3 sunDir = normalize(vec3(0.36, 0.06, 0.48));
	vec3 H = normalize(V + sunDir);
	float spec = pow(max(dot(n, H), 0.0), 90.0);
	col += vec3(1.0, 0.75, 0.55) * spec * 1.4 * smoothstep(0.3, 0.9, w2);

	// Distance haze toward the horizon colour.
	float dist = length(ex_world - cam_pos.xyz);
	col = mix(col, horizon, (1.0 - exp(-dist * 0.010)) * 0.95);
	fragColor = vec4(col, 1.0);
}
