#version 150
precision highp float;

// EXT: one axis of a separable Gaussian (src/ext/postfx.rs, step 2). `step_uv` is one texel
// across or one texel down; the pass runs twice with the two.
//
// Nine taps, but only five fetches: each off-centre tap sits BETWEEN two texels at a weighted
// offset, so the hardware's bilinear filter sums that pair for free. The offsets and weights
// below are the standard reduction of the 9-tap binomial kernel.

uniform sampler2D tex;
uniform vec2 step_uv;

const float OFFSET[3] = float[](0.0, 1.3846153846, 3.2307692308);
const float WEIGHT[3] = float[](0.2270270270, 0.3162162162, 0.0702702703);

in vec2 ex_uv;
out vec4 fragColor;

void main(void) {
	vec3 c = texture(tex, ex_uv).rgb * WEIGHT[0];
	for (int i = 1; i < 3; ++i) {
		vec2 d = step_uv * OFFSET[i];
		c += texture(tex, ex_uv + d).rgb * WEIGHT[i];
		c += texture(tex, ex_uv - d).rgb * WEIGHT[i];
	}
	fragColor = vec4(c, 1.0);
}
