#version 150
precision highp float;

//Inputs
uniform sampler2D tex;
//EXT: a colour mixed over the far side by its alpha; zero alpha is the original look.
uniform vec4 tint;
in vec4 ex_uv;

//Outputs
out vec4 fragColor;

void main(void) {
	vec2 uv = (ex_uv.xy / ex_uv.w);
	uv = uv*0.5 + 0.5;
	vec3 view = texture(tex, uv).rgb;
	fragColor = vec4(mix(view, tint.rgb, tint.a), 1.0);
}
