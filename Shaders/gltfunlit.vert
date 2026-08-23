#version 150

// EXT: vertex stage for UNLIT glTF geometry (KHR_materials_unlit) loaded by
// src/ext/gltf_model.rs -- the backrooms scan, whose lighting is baked into its maps.
//
// ATTRIBUTE ORDER IS LOAD-BEARING. Shader::new scrapes this source for "\nin " and binds the
// names to locations 0, 1, 2, ... in declaration order (shader.rs:88-96), and gltf_model.rs
// fills locations 0..3 as in_pos, in_uv, in_normal, in_tangent. Only the first two are
// consumed here, so only those two are declared: the VAO's normal and tangent streams at 2
// and 3 are simply never read, and declaring them would bind nothing differently.

uniform mat4 mvp;
uniform mat4 model;   // local_to_world, for the fog distance (set by GltfModel::draw_part)

in vec3 in_pos;
in vec2 in_uv;

out vec2 ex_uv;
out vec3 ex_world;

void main(void) {
	gl_Position = mvp * vec4(in_pos, 1.0);
	ex_uv = in_uv;
	ex_world = (model * vec4(in_pos, 1.0)).xyz;
}
