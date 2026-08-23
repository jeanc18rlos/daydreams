#version 150
precision highp float;
// EXT: translucent hologram of a held object at its unconstrained placement (src/ext/grab.rs).
// Same lighting as texture.frag, lifted toward white and drawn at alpha 0.35 with blending on.

#define LIGHT vec3(0.36, 0.80, 0.48)
#define ALPHA 0.35
#define TINT 0.45

//Inputs
uniform sampler2D tex;
in vec2 ex_uv;
in vec3 ex_normal;

//Outputs
out vec4 fragColor;

void main(void) {
	float s = dot(ex_normal, LIGHT)*0.5 + 0.5;
	vec3 lit = texture(tex, ex_uv).rgb * s;
	vec3 holo = mix(lit, vec3(1.0), TINT);
	fragColor = vec4(holo, ALPHA);
}
