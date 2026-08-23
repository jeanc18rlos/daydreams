#version 150
precision highp float;

// EXT: textured + tinted, with real alpha. Drawn with blending enabled by ext/ui.rs.
uniform sampler2D tex;
uniform vec4 color;
in vec2 ex_uv;

out vec4 fragColor;

void main(void) {
	fragColor = texture(tex, ex_uv) * color;
}
