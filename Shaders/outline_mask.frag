#version 150
precision highp float;

// EXT: every covered pixel is 1 in the single-channel mask.
out vec4 fragColor;

void main(void) {
	fragColor = vec4(1.0);
}
