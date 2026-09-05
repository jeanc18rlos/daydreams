#version 150
precision highp float;

// EXT: textured + tinted, with real alpha. Drawn with blending enabled by ext/ui.rs.
//
// The tint is a GRADIENT between two colours, taken along `grad_axis`. Ordinary draws pass the
// same colour twice and a zero axis, so they are the flat tint they always were; the title
// screen's scrim passes two and an axis, and gets a smooth wash across the frame from one
// draw instead of the strip of forty rectangles that would otherwise be needed to avoid
// banding at 8 bits.
uniform sampler2D tex;
uniform vec4 color;
uniform vec4 color2;
uniform vec2 grad_axis;
in vec2 ex_uv;
in vec2 ex_quad;

out vec4 fragColor;

void main(void) {
	float t = clamp(dot(ex_quad, grad_axis), 0.0, 1.0);
	fragColor = texture(tex, ex_uv) * mix(color, color2, t);
}
