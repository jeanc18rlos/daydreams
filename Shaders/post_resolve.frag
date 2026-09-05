#version 150
precision highp float;

// EXT: the composite (src/ext/postfx.rs, step 3): scene, plus bloom, plus the two things that
// make the title screen read as a memory of a place rather than a photograph of one.
//
//   VEIL. The bloom is added twice over. Once as light, straight on top; and once as a
//   *veil* -- the scene lifted TOWARD the bloom's colour, most where the scene is darkest.
//   That second use is the dreamlike part, and it is physical: light scattering off the air
//   between you and a bright thing does not brighten the shadows evenly, it washes them out,
//   which is why distant hills go pale near a low sun and why a lit doorway seems to soften
//   the grass in front of it. Adding light alone leaves the shadows crushed and the frame
//   reads as a game; veiling them is what opens it up.
//
//   VIGNETTE. A gentle darkening at the corners. It is the oldest trick there is for making a
//   frame read as a photograph, and here it does a second job: the menu's own wash is a
//   gradient from the left (`ext/menu.rs`), and the vignette closes the other three sides so
//   the type sits inside something rather than on top of it.

uniform sampler2D tex;    // the scene at full resolution
uniform sampler2D tex2;   // the blurred bright pass, at a quarter of it

#define BLOOM 0.13
#define VEIL 0.10
#define VIGNETTE 0.22

in vec2 ex_uv;
out vec4 fragColor;

// Same shoulder as Shaders/sky.frag: the sum below routinely passes 1 where the bloom lands on
// something already bright, and a hard clip would take those pixels to white in the order
// red, green, blue -- turning the warm core of the glow into a flat white blob.
#define KNEE 0.80
vec3 shoulder(vec3 c) {
	vec3 over = max(c - vec3(KNEE), vec3(0.0));
	return min(c, vec3(KNEE)) + (1.0 - KNEE) * (over / (over + (1.0 - KNEE)));
}

float dither(vec2 p) {
	return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453) - 0.5;
}

void main(void) {
	vec3 scene = texture(tex, ex_uv).rgb;
	vec3 glow = texture(tex2, ex_uv).rgb;

	// Veil first, so the light added next sits on top of an already-opened image rather than
	// being flattened into it. `1 - luminance` puts nearly all of it in the shadows.
	float dark = 1.0 - clamp(dot(scene, vec3(0.3, 0.59, 0.11)), 0.0, 1.0);
	float amount = clamp(dot(glow, vec3(0.3, 0.59, 0.11)) * 1.6, 0.0, 1.0);
	vec3 col = mix(scene, max(scene, glow), amount * dark * VEIL);
	col += glow * BLOOM;

	// Vignette, as a smooth radial falloff on a corrected radius so it stays circular at any
	// aspect rather than becoming an ellipse on a wide panel.
	//
	// The far end of the ramp is the CORNER's own radius, derived from the aspect rather than
	// fixed, so full strength lands at the corner and nowhere else whatever the window is.
	// Fixed at 0.62 it saturated a third of the way out, leaving a wide flat band at full
	// strength with the corner no darker than the mid-edge -- which reads as a dark ring
	// drawn around the picture rather than as a lens, whose falloff is monotonic from the
	// axis outward.
	float aspect = float(textureSize(tex, 0).y) / float(textureSize(tex, 0).x);
	vec2 d = (ex_uv - 0.5) * vec2(1.0, aspect);
	col *= 1.0 - VIGNETTE * smoothstep(0.0, 1.0 + aspect * aspect, dot(d, d) * 4.0);

	fragColor = vec4(shoulder(col) + dither(gl_FragCoord.xy) * (1.0 / 255.0), 1.0);
}
