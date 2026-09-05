#version 150
precision highp float;

// EXT: unlit glTF material (KHR_materials_unlit), for src/ext/gltf_model.rs.
//
// The backrooms model is a light-bake: every wall, lamp and carpet already carries its final
// colour in its map, and KHR_materials_unlit is the file saying "do not light this". So this
// shader adds nothing the ported light direction, the weather grade or the door's light pool
// would otherwise contribute -- `mood` is deliberately ignored, the place is its own weather.
//
// What it does add is a mild depth fog toward `fog_color`, for two reasons: the bake has no
// atmosphere of its own, so a corridor's far end reads as flat as its near end; and the
// engine's far plane is 100 units (GH_FAR) while the maze is 80 long, so without it the
// furthest walls would pop at the clip rather than fade. The colour is the caller's because
// two things draw with this shader and they must fade to different places: the building's
// walls to a dark yellow-brown of their own, and the ground cap under it (GroundCap in
// src/ext/backrooms.rs) to the interior sky's horizon tone, exactly -- anything else leaves a
// bright line where the cap's far edge meets the sky. Those two -- `Backrooms::draw` and
// `GroundCap::draw` -- are the only things that draw with this shader, and each sets
// `fog_color` every draw. Left unset it is GL's zero, black: a far end that fades to black
// is the sign of a new caller that forgot it.

uniform sampler2D tex;    // base colour map as shipped (1x1 white when the material has none)
uniform vec4 base_color;  // glTF baseColorFactor
uniform vec4 emissive;    // emissiveFactor x KHR_materials_emissive_strength, unclamped
uniform vec4 fog_color;   // what the far end fades to; rgb used
uniform vec4 cam_pos;
uniform float alpha_cutoff; // discard below this map alpha; negative = no test

in vec2 ex_uv;
in vec3 ex_world;

// EXT: the flashlight's cone (src/ext/view.rs publishes these; ext/tool.rs aims it).
// This shader draws a BAKED interior and has no normals by design ("the place is its own
// weather", above) -- so the torch is applied flat: it brightens what the beam finds without
// pretending to relight it. That is deliberately a reversal of this shader's own rule, and it
// is the one light the bake cannot already contain, because the player is carrying it.
uniform vec4 spot_pos;   // xyz world origin, w = range in metres (0 = off, and GL's default)
uniform vec4 spot_dir;   // xyz unit direction, w = cos(outer angle)
uniform vec4 spot_col;   // rgb radiance, w = cos(inner angle)

vec3 spot_flat(vec3 P) {
	if (spot_pos.w <= 0.0) return vec3(0.0);
	vec3 d = spot_pos.xyz - P;
	float r = length(d);
	if (r >= spot_pos.w) return vec3(0.0);
	vec3 L = d / max(r, 1e-4);
	float cone = smoothstep(spot_dir.w, spot_col.w, dot(-L, spot_dir.xyz));
	if (cone <= 0.0) return vec3(0.0);
	float atten = pow(max(1.0 - r / spot_pos.w, 0.0), 2.2);
	return spot_col.rgb * cone * atten;
}

out vec4 fragColor;

void main(void) {
	vec4 map = texture(tex, ex_uv);
	// The alpha test (src/ext/gltf_model.rs, "Alpha"): the map's alpha as shipped times the
	// factor's, against the material's cutoff.
	float a = map.a * base_color.a;
	if (a < alpha_cutoff) {
		discard;
	}
	// There is no HDR target here, so a strength of 10 means "saturate": the red exit lamps and
	// the white diffusers clip to pure colour, which is what they look like in the reference.
	vec3 col = clamp(map.rgb * base_color.rgb + emissive.rgb, 0.0, 1.0);

	// Squared-distance falloff rather than linear: nothing within arm's reach is touched
	// (1% at 5 units), the 23-unit hall end is softened (19%), and the far end of the maze
	// is mostly gone (92% at 80) before the far plane would have cut it.
	// The torch, before the fog: a beam does not punch through haze, it lights what is in it.
	vec3 sp = spot_flat(ex_world);
	col += col * sp * 1.35 + sp * 0.055;

	float d = length(ex_world - cam_pos.xyz) * 0.0195;
	float fog = 1.0 - exp(-d * d);
	col = mix(col, fog_color.rgb, fog);

	// The alpha only matters in the translucent pass, where blending is on.
	fragColor = vec4(col, a);
}
