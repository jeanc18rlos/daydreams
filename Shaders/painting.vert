#version 150

// EXT: vertex stage for the Backrooms' portraits (src/ext/painting.rs). The canvas is the
// ported quad.obj, so the streams are the OBJ loader's: positions, then uvs, then the flat
// normals, which this shader never reads and so never declares.
//
// ATTRIBUTE ORDER IS LOAD-BEARING. Shader::new scrapes this source for "\nin "
// (shader.rs, `scrape_attribs`) and binds the names to locations 0, 1, 2, ... in declaration
// order (its `bind_attrib_location` loop); Mesh::new fills location 0 with positions and 1
// with uvs.

uniform mat4 mvp;
uniform mat4 model;   // local_to_world, for the fog distance (the same term gltfunlit uses)

in vec3 in_pos;
in vec2 in_uv;

out vec2 ex_uv;
out vec3 ex_world;

void main(void) {
	gl_Position = mvp * vec4(in_pos, 1.0);
	ex_uv = in_uv;
	ex_world = (model * vec4(in_pos, 1.0)).xyz;
}
