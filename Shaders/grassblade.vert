#version 150

// EXT: grass blades. All bending happens here -- the patch mesh is static and the CPU never
// touches a vertex.
//
// in_uv carries per-vertex blade data packed by src/ext/grassgen.rs:
//   .x = t, height along the blade (0 root .. 1 tip)
//   .y = per-blade random, so neighbours sway out of phase
//   .z = lean angle, the direction this blade bends
//
// Not the engine's mvp/mv pair: the blades bend in WORLD space (the wind field must not travel
// with the patch, which follows the player), so the patch's local-to-world and the camera's
// view-projection are wanted separately. Sending both is what lets the shader skip the two 4x4
// inversions per vertex it used to spend reconstructing them from mv.
uniform mat4 vp;          // projection * world_view (Camera::matrix)
uniform mat4 l2w;         // the patch object's local_to_world
uniform float time;
uniform vec4 cam_pos;
uniform float wrap;       // world period on a wrapping scene, 0 otherwise (src/ext/view.rs)

// ── Toroidal meadow support. See src/ext/terrain.rs for what this is and why.
//
// These constants MIRROR ext::terrain: AMP, PAD_R and the door position. `terrain_constants_match`
// in that module fails the build if they drift, because blades sitting on a different surface
// than the one being drawn is not something a screenshot always shows.
#define T_AMP   8.0
#define T_DOOR  vec2(0.0, -6.0)
#define T_CLEAR_R 70.0
#define T_CREST_H 8.0
#define T_CREST_R 55.0

// Shortest signed separation on a circle of circumference `wrap`.
float wrap_delta(float d) {
	float h = wrap * 0.5;
	return mod(d + h, wrap) - h;
}

// Ground height, the same sum of whole-cycle harmonics ext::terrain::height evaluates on the CPU
// to build the terrain mesh and its colliders. Blades read it so they stand ON the hills; without
// it they hover in a flat sheet while the ground rolls underneath.
float terrain_h(vec2 p) {
	if (wrap <= 0.0) {
		return 0.0;
	}
	float u = p.x * (6.28318530718 / wrap);
	float v = p.y * (6.28318530718 / wrap);
	float h = 0.55 * sin(u) * cos(v)
	        + 0.28 * sin(2.0 * u + 1.7) * cos(3.0 * v - 0.4)
	        + 0.17 * sin(3.0 * u - 0.9) * cos(v + 2.1);
	vec2 d = vec2(wrap_delta(p.x - T_DOOR.x), wrap_delta(p.y - T_DOOR.y));
	float d2 = dot(d, d);
	float clear = exp(-d2 / (T_CLEAR_R * T_CLEAR_R));
	float crest = T_CREST_H * exp(-d2 / (T_CREST_R * T_CREST_R));
	return T_AMP * h * (1.0 - clear) + crest;
}

// Round a world-space frequency to a whole number of cycles per world period, so wrapping the
// player by one period cannot land the wind field mid-cycle. Identity when wrap = 0.
float snap_freq(float f) {
	return wrap > 0.0 ? max(1.0, floor(f * wrap + 0.5)) / wrap : f;
}

// The same idea for a raw phase multiplier (radians per world unit rather than cycles): round it
// to a whole number of 2*pi turns per period.
float snap_phase(float k) {
	return wrap > 0.0 ? max(1.0, floor(k * wrap / 6.28318530718 + 0.5)) * 6.28318530718 / wrap : k;
}

in vec3 in_pos;
in vec3 in_uv;

out vec3 ex_world;
out vec3 ex_normal;
out float ex_t;           // height along blade, for AO and colour
out float ex_rand;

// Cheap value noise; the wind field only needs to be smooth, not detailed.
float hash12(vec2 p) {
	vec3 p3 = fract(vec3(p.xyx) * 0.1031);
	p3 += dot(p3, p3.yzx + 33.33);
	return fract((p3.x + p3.y) * p3.z);
}
// `period` is the noise lattice's repeat, in lattice cells; <= 0 means do not repeat.
//
// Wrapping the LATTICE INDEX is the part that is easy to miss. Snapping the sample frequency to
// a whole number of cycles per world period makes the sample point land on an equivalent lattice
// cell after a wrap -- but hash12(i) and hash12(i + period) are unrelated random values, so the
// wind field would still jump. Every blade within a few metres of the camera would snap to a new
// lean in one frame, which is far more obvious than any of the distant tiling artifacts.
float vnoise(vec2 p, float period) {
	vec2 i = floor(p), f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	vec2 i0 = i, i1 = i + 1.0;
	if (period > 0.0) {
		i0 = mod(i0, vec2(period));
		i1 = mod(i1, vec2(period));
	}
	return mix(mix(hash12(i0), hash12(vec2(i1.x, i0.y)), f.x),
	           mix(hash12(vec2(i0.x, i1.y)), hash12(i1), f.x), f.y);
}

void main(void) {
	float t = in_uv.x;
	float rnd = in_uv.y;
	float lean = in_uv.z;

	// The patch object is moved by ext/grassfield.rs, so local -> world gives the true
	// world position the wind field must be sampled at (otherwise gusts would travel with
	// the player).
	vec3 root = (l2w * vec4(in_pos, 1.0)).xyz;
	vec3 world = root;

	// Gust field: two scales drifting downwind, plus a fast flutter per blade.
	vec2 wdir = normalize(vec2(0.82, 0.57));
	float f1 = snap_freq(0.055), f2 = snap_freq(0.180);
	float g1 = vnoise(world.xz * f1 - wdir * time * 0.55, f1 * wrap);
	float g2 = vnoise(world.xz * f2 - wdir * time * 1.10, f2 * wrap);
	float gust = (g1 - 0.5) * 1.5 + (g2 - 0.5) * 0.7;
	float flutter = sin(time * 6.0 + rnd * 31.0 + world.x * snap_phase(0.7)) * 0.12;

	// Bend grows with t^2: the root stays planted, the tip travels. This is what separates
	// grass from a swaying billboard.
	float bend = (gust * 0.55 + flutter) * t * t;
	vec3 sway = vec3(wdir.x, 0.0, wdir.y) * bend;
	// A little lean-direction wobble so the field does not comb perfectly flat.
	sway += vec3(cos(lean), 0.0, sin(lean)) * bend * 0.35;
	world += sway;
	// Bending shortens the blade slightly, as a real one pivots rather than stretches.
	world.y -= length(sway) * 0.35 * t;
	// Stand the blade on the terrain. Sampled at the blade's ROOT (its unswayed xz) rather than
	// at this vertex, so the whole blade rises together instead of shearing along a slope.
	world.y += terrain_h(root.xz);

	gl_Position = vp * vec4(world, 1.0);
	ex_world = world;
	ex_t = t;
	ex_rand = rnd;

	// Blade normal: perpendicular to the blade's face, tilted by the bend. Flat per-face
	// normals from the engine would make every quad a different tone, so we build our own.
	vec3 n = normalize(vec3(-sin(lean), 0.85, cos(lean)) + vec3(sway.x, 0.0, sway.z) * 1.5);
	ex_normal = n;
}
