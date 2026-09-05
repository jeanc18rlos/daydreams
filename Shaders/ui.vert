#version 150

// EXT: screen-space UI quad. `mvp` is a pure scale/translate built by ext/ui.rs that maps the
// unit quad straight into NDC; w stays 1, so no projection is involved.
uniform mat4 mvp;
uniform mat4 mv;
// Sub-rectangle of the atlas to sample, as (u0, v0, u1, v1) with v measured from the TOP of
// the image -- the engine's BMP loader leaves GL t=0 at the image top (Texture.cpp:27-41).
uniform vec4 uv_rect;

in vec3 in_pos;
in vec2 in_uv;
in vec3 in_normal;

out vec2 ex_uv;
// Position within the QUAD, top-left origin, independent of which part of the atlas is being
// sampled. Only the gradient fill uses it (ext/ui.rs `fill_rect_grad`).
out vec2 ex_quad;

void main(void) {
	gl_Position = mvp * vec4(in_pos, 1.0);
	// quad.obj's vt has v=1 at the TOP vertex; with t=0 = image top that is upside down, so
	// flip v once here and map into the requested rectangle.
	vec2 q = vec2(in_uv.x, 1.0 - in_uv.y);
	ex_uv = mix(uv_rect.xy, uv_rect.zw, q);
	ex_quad = q;
}
