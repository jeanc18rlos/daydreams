#version 150
precision highp float;

// EXT: bloom's bright pass (src/ext/postfx.rs, step 1). Runs at a quarter resolution, so each
// of the four taps below is itself a bilinear average of four full-resolution pixels: sixteen
// pixels reduced per output pixel, which is what keeps a small bright thing -- the sunset in a
// doorway -- from flickering as it moves behind the grass.
//
// The knee is soft rather than a hard threshold. A hard one makes bloom pop in and out as a
// highlight crosses it, and puts a visible contour along the edge of every bright region.

uniform sampler2D tex;

#define KNEE 0.86
#define SOFT 0.12

in vec2 ex_uv;
out vec4 fragColor;

void main(void) {
	vec2 texel = 1.0 / vec2(textureSize(tex, 0));
	vec3 c = texture(tex, ex_uv + texel * vec2(-1.0, -1.0)).rgb;
	c += texture(tex, ex_uv + texel * vec2(1.0, -1.0)).rgb;
	c += texture(tex, ex_uv + texel * vec2(-1.0, 1.0)).rgb;
	c += texture(tex, ex_uv + texel * vec2(1.0, 1.0)).rgb;
	c *= 0.25;

	// Weight by how far the brightest channel is past the knee, not by luminance: a saturated
	// orange sun is bright light even though its luminance is middling, and weighting by
	// luminance would bloom a white wall harder than the sunset.
	float peak = max(c.r, max(c.g, c.b));
	float w = smoothstep(KNEE, KNEE + SOFT, peak);
	fragColor = vec4(c * w, 1.0);
}
