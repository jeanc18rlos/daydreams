#version 150

//Globals
uniform mat4 mvp;
uniform mat4 mv;

//Inputs
in vec3 in_pos;
in vec2 in_uv;

//Outputs
out vec3 ex_normal;

void main(void) {
	// EXT: the quad sits at the far plane (NDC z = 1) and is drawn LAST under GL_LEQUAL, so it
	// fills only what nothing else covered; the original drew it first at z = 0 with the depth
	// mask off and let the scene overdraw it (Sky.h:12-20). The view ray is still taken through
	// the z = 0 point, exactly as before -- the ray's direction does not depend on which depth
	// the quad is rasterised at, only on the inverse projection of its screen position.
	vec4 ndc = vec4(in_pos.xy, 0.0, 1.0);
	gl_Position = vec4(in_pos.xy, 1.0, 1.0);
	vec3 eye_normal = normalize((mvp * ndc).xyz);
	ex_normal = normalize((mv * vec4(eye_normal, 0.0)).xyz);
}
