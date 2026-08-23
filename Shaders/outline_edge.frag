#version 150
precision highp float;

// EXT: contour extraction. A pixel OUTSIDE the silhouette mask that sees the mask anywhere
// within WIDTH_PX pixels is on the outline. params = (1/width, 1/height, width_px, unused).
uniform sampler2D tex;
uniform vec4 params;
in vec2 ex_uv;

out vec4 fragColor;

#define TAPS 16

void main(void) {
	float here = texture(tex, ex_uv).r;
	if (here > 0.5) {
		discard; // inside the object: the object itself is drawn there
	}
	vec2 texel = params.xy;
	float r = params.z;
	float hit = 0.0;
	// Two rings (r and r/2) so thin features are not missed between taps.
	for (int i = 0; i < TAPS; ++i) {
		float a = 6.2831853 * float(i) / float(TAPS);
		vec2 d = vec2(cos(a), sin(a)) * texel;
		hit = max(hit, texture(tex, ex_uv + d * r).r);
		hit = max(hit, texture(tex, ex_uv + d * r * 0.5).r);
	}
	if (hit < 0.5) {
		discard;
	}
	fragColor = vec4(1.0, 1.0, 1.0, 1.0);
}
