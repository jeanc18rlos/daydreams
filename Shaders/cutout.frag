#version 150
precision highp float;

// EXT: not part of the original shader set.
//
// Draws a baked image on a flat quad with a black key colour punched out. The engine has no
// alpha blending anywhere, so transparency has to be a discard.
//
// Two deliberate differences from texture.frag:
//   1. NO lighting term. The image is pre-lit by the baker using this shader's own LIGHT
//      constant, so applying it again would double-shade it.
//   2. V is flipped. Texture.cpp reads BMP rows bottom-first into img[height-1] downward
//      (Texture.cpp:27-41), so img row 0 ends up holding the image's TOP row -- and OpenGL
//      treats row 0 as t=0, the bottom of texture space. Net effect: every texture in this
//      engine is stored upside down. Harmless for the original's symmetric checkerboards;
//      very much not harmless for a baked projection.

uniform sampler2D tex;
in vec2 ex_uv;

out vec4 fragColor;

void main(void) {
	vec3 c = texture(tex, vec2(ex_uv.x, 1.0 - ex_uv.y)).rgb;
	if (c.r + c.g + c.b < 0.06) {
		discard;
	}
	fragColor = vec4(c, 1.0);
}
