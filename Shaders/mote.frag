#version 150
precision highp float;

// EXT: one mote of dust (src/ext/doorlight.rs). A soft round spark, added into the frame.
//
// The falloff is quartic rather than linear so the mote has a bright core and a long faint
// skirt -- which is what an out-of-focus point of light looks like, and what keeps a few
// hundred of these from reading as a spray of hard dots.

#define MOTE_COL vec3(1.00, 0.78, 0.46)

in float ex_fade;
out vec4 fragColor;

void main(void) {
	// gl_PointCoord runs 0..1 across the point, origin at its top-left corner.
	vec2 d = gl_PointCoord * 2.0 - 1.0;
	float r2 = dot(d, d);
	if (r2 > 1.0) {
		discard;
	}
	float falloff = 1.0 - r2;
	falloff *= falloff;
	fragColor = vec4(MOTE_COL, falloff * ex_fade * 0.95);
}
