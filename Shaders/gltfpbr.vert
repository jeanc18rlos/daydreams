#version 150

// EXT: vertex stage for glTF geometry loaded by src/ext/gltf_model.rs.
//
// Unlike every ported shader, this one receives the model's OWN smooth normals and tangents
// rather than flat per-face normals recomputed by the OBJ loader (Mesh.cpp:187). That single
// difference is what turns a faceted slab back into rounded joinery.
//
// ATTRIBUTE ORDER IS LOAD-BEARING. Shader::new scrapes this source for "\nin " and binds the
// names to locations 0, 1, 2, ... in declaration order (shader.rs:88-96), and gltf_model.rs
// fills locations 0..3 in exactly this order. Reordering these four lines silently swaps the
// vertex streams.

uniform mat4 mvp;
uniform mat4 mv;   // = transpose(world_to_local): the NORMAL matrix, so it yields world space

in vec3 in_pos;
in vec2 in_uv;
in vec3 in_normal;
in vec4 in_tangent;   // xyz tangent, w = bitangent handedness (glTF convention)

out vec2 ex_uv;
out vec3 ex_normal;
out vec3 ex_tangent;
out vec3 ex_bitangent;
out vec3 ex_world;

void main(void) {
	gl_Position = mvp * vec4(in_pos, 1.0);
	ex_uv = in_uv;

	// mv is inverse-transpose(local_to_world), so this is a world-space normal -- the space
	// LIGHT is defined in. The name is a misnomer inherited from the port (object.rs:83).
	ex_normal = normalize((mv * vec4(in_normal, 0.0)).xyz);

	// Tangents are DIRECTIONS along the surface, not normals: they transform by local_to_world,
	// not by its inverse-transpose. The two agree only under uniform scale, and the door is
	// unscaled, but taking the correct one costs nothing here (this model is 2852 vertices).
	mat4 l2w = inverse(transpose(mv));
	ex_tangent = normalize((l2w * vec4(in_tangent.xyz, 0.0)).xyz);
	ex_bitangent = cross(ex_normal, ex_tangent) * in_tangent.w;

	ex_world = (l2w * vec4(in_pos, 1.0)).xyz;
}
