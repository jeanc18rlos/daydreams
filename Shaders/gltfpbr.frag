#version 150
precision highp float;

// EXT: material shading for glTF geometry (src/ext/gltf_model.rs).
//
// The engine has exactly one light -- a hard-coded world-space direction shared by every ported
// shader -- and no ambient term, no specular and no sRGB anywhere. This shader stays inside that
// world rather than importing a real PBR pipeline: it adds only what the ported `texture` shader
// cannot express and the model actually carries, namely tangent-space normals, per-material
// metalness, occlusion and emission. Then it applies the SAME weather grade and light pool as
// the ground and grass, so glTF geometry sits in the scene instead of on top of it.
//
// INTERIORS (mood > 1.5) are the exception: a Sketchfab room was authored for a lit scene, not
// baked, and the outdoor sun would light its ceiling from below and its far wall at noon. Inside,
// the sun is replaced by a hemisphere -- warm white from above, a dim brown bounce from the
// floor -- a small constant ambient, a damped overhead specular, and the material's own
// emission, which is what the ceiling lights and exit signs are made of.

#define LIGHT vec3(0.36, 0.80, 0.48)
// The interior hemisphere: what faces up is lit by the room's lamps, what faces down by the
// floor's bounce. Plus a floor of light nothing falls below, so a cupboard's underside is dim
// rather than black.
#define HEMI_UP vec3(0.90, 0.864, 0.792)
#define HEMI_DOWN vec3(0.25, 0.22, 0.18)
#define AMBIENT vec3(0.06)

uniform sampler2D tex;    // RGB = base colour, A = base alpha (1 for an OPAQUE material)
uniform sampler2D tex2;   // RG = tangent-space normal xy, B = roughness, A = metalness
uniform sampler2D tex3;   // RGB = emissive (factor and strength applied), A = ambient occlusion
uniform vec4 cam_pos;
uniform float mood;       // -1 daylight, 0 storm, 1 sunset, 2 interior (src/ext/view.rs)
uniform vec4 glow;        // intro door light pool (xyz, strength)
uniform float detail;     // 1 in the main view, 0 inside a portal framebuffer
uniform float alpha_cutoff; // discard below this base alpha; negative = no test

in vec2 ex_uv;
in vec3 ex_normal;
in vec3 ex_tangent;
in vec3 ex_bitangent;
in vec3 ex_world;

out vec4 fragColor;

void main(void) {
	vec4 alb = texture(tex, ex_uv);
	// The alpha test, before any lighting is paid for. A BLEND material not named translucent
	// and a MASK material both come through here (src/ext/gltf_model.rs, "Alpha").
	if (alb.a < alpha_cutoff) {
		discard;
	}
	vec4 srf = texture(tex2, ex_uv);
	vec4 light = texture(tex3, ex_uv);
	vec3 base = alb.rgb;
	float ao = light.a;
	float rough = clamp(srf.b, 0.06, 1.0);
	float metal = srf.a;

	vec3 n = normalize(ex_normal);
	vec3 V = normalize(cam_pos.xyz - ex_world);

	// The ported renderer re-draws the whole scene into each portal's framebuffer, up
	// to four levels deep (engine.cpp:207-270), so anything done here is paid for several times
	// over for a result that ends up a small quad on screen. Normal mapping and the specular
	// lobe are the two costly parts and the two least visible there, so a portal pass skips
	// both and shades off the interpolated vertex normal alone.
	if (detail > 0.5) {
		// Tangent-space normal. Only xy is stored; z is reconstructed, which is exact for a
		// unit normal and frees a channel for roughness.
		vec2 nxy = srf.rg * 2.0 - 1.0;
		float nz = sqrt(max(1.0 - dot(nxy, nxy), 0.0));
		vec3 T = normalize(ex_tangent);
		vec3 B = normalize(ex_bitangent);
		n = normalize(nxy.x * T + nxy.y * B + nz * n);
	}

	// Back faces: the engine enables CULL_FACE once at init and a scene cannot turn it off
	// (engine.rs:114-116), but a swinging door still shows its far side through the portal
	// opening. Flip toward the viewer so a reversed winding never lights as though it faced away.
	if (!gl_FrontFacing) {
		n = -n;
	}

	// Blinn-Phong standing in for a GGX lobe: same shape where it matters, a fraction of the
	// cost, and there is no environment map here for a real one to sample anyway. Grazing
	// angles reflect more, whatever the material -- this is what stops painted surfaces
	// reading as matte cardboard.
	float gloss = exp2(mix(4.0, 11.0, 1.0 - rough));
	float f = 0.04 + 0.96 * pow(clamp(1.0 - max(dot(n, V), 0.0), 0.0, 1.0), 5.0);
	vec3 tint = mix(vec3(1.0), base, metal);
	vec3 R = reflect(-V, n);

	vec3 col;
	if (mood > 1.5) {
		// INTERIOR: hemisphere plus ambient on the diffuse; metals have no diffuse and
		// reflect the same hemisphere instead. The specular comes from straight overhead
		// -- the ceiling's lamps -- and is damped, because a room's light is broad and its
		// highlights soft.
		vec3 hemi = mix(HEMI_DOWN, HEMI_UP, n.y * 0.5 + 0.5) + AMBIENT;
		col = base * (1.0 - metal) * hemi * ao;
		if (detail > 0.5) {
			vec3 H = normalize(vec3(0.0, 1.0, 0.0) + V);
			float spec = pow(max(dot(n, H), 0.0), gloss);
			col += tint * spec * mix(f, 1.0, metal) * (1.0 - rough * 0.7) * 0.35;
			vec3 env = mix(HEMI_DOWN, HEMI_UP, clamp(R.y * 0.5 + 0.5, 0.0, 1.0));
			col += base * env * metal * mix(0.35, 1.0, 1.0 - rough) * ao;
		}
	} else {
		vec3 L = normalize(LIGHT);
		// Wrap lighting, matching Shaders/texture.frag:15 -- a hard terminator would make
		// this the only object in the game with one.
		float ndl = dot(n, L) * 0.5 + 0.5;
		vec3 sky = vec3(0.42, 0.52, 0.62);

		// Metals have no diffuse; dielectrics keep their colour. Painted joinery is a
		// dielectric, the lever handle is not, and that difference is the whole reason for
		// carrying metalness.
		vec3 diffuse = base * (1.0 - metal) * (ndl * 0.95 + 0.20);
		diffuse += base * (1.0 - metal) * sky * 0.18;

		col = diffuse * ao;

		if (detail > 0.5) {
			vec3 H = normalize(L + V);
			float spec = pow(max(dot(n, H), 0.0), gloss);
			col += tint * spec * mix(f, 1.0, metal) * (1.0 - rough * 0.7);
			// A metal has NO diffuse, so with nothing to reflect it renders black -- which
			// is what happened to the lever handle. There is no environment map in this
			// engine, so fake one: a ground colour below, sky above, picked by the
			// reflected ray's height. Rough metal still reflects, just blurrily, so the
			// falloff must not go to zero with roughness -- that alone was most of why the
			// handle was a black smudge.
			vec3 env = mix(vec3(0.26, 0.28, 0.30), sky * 1.7, clamp(R.y * 0.5 + 0.5, 0.0, 1.0));
			col += base * env * metal * mix(0.35, 1.0, 1.0 - rough) * 1.5 * ao;
		}
	}

	// Emission: factor and strength were applied and clamped at load, so a strength of 10
	// simply saturates -- there is no HDR target for it to mean more than that.
	col += light.rgb;

	// The open door spills warm light around itself. Applied BEFORE the weather grade and
	// multiplied into the existing colour, so it reads as light in the scene rather than yellow
	// paint over an already-darkened surface -- same rule as Shaders/grassblade.frag:76-83.
	if (glow.w > 0.0) {
		float gd = length(ex_world - glow.xyz);
		float pool = glow.w * pow(max(1.0 - gd / 3.5, 0.0), 2.0);
		col += col * vec3(1.00, 0.86, 0.62) * pool * 0.9;
	}

	// Weather grade and distance haze. Same illuminant and the same haze colours as the ground
	// and grass, but NOT the same multiplier: theirs is green-biased (grass.frag, grassblade.frag
	// use 0.46,0.56,0.48) because it was tuned on a green surface, where the bias is invisible.
	// On white paint it is not -- it turned the door sage. Storm light is cool and grey, so this
	// dims toward blue instead, which is the same weather read correctly on a neutral surface.
	vec3 haze = vec3(0.74, 0.82, 0.90);
	if (mood > 1.5) {
		// INTERIOR: no grade at all -- the Backrooms' return door is lit by the building's own
		// lamps (the hemisphere above), and the only thing a sunset grade did to it was turn
		// its white paint pink. The haze goes toward the dark warm tone the unlit walls fade
		// to (Shaders/gltfunlit.frag), so the door sits in the same air as the hall around it.
		haze = vec3(0.10, 0.08, 0.05);
	} else if (mood > 0.5) {
		col *= vec3(1.02, 0.80, 0.68);
		haze = vec3(0.95, 0.62, 0.52);
	} else if (mood > -0.5) {
		col = mix(col, vec3(dot(col, vec3(0.3, 0.59, 0.11))), 0.10) * vec3(0.62, 0.66, 0.74);
		haze = vec3(0.30, 0.34, 0.38);
	}

	float dist = length(ex_world - cam_pos.xyz);
	float fog = 1.0 - exp(-dist * 0.0080);
	col = mix(col, haze, fog * 0.9);

	// The alpha only matters in the translucent pass, where blending is on; everywhere else
	// the framebuffer ignores it.
	fragColor = vec4(col, alb.a);
}
