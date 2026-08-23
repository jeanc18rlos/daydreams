#version 150
precision highp float;

// EXT: a painted portrait whose eyes follow the viewer, for src/ext/painting.rs.
//
// The portrait is two textures made by tools/gen_portraits.py from a hand-edited sheet of a
// public-domain painting (src/ext/portrait_atlas.rs holds the numbers): `base` is the sitter
// with blank eye sockets and no mouth, and `parts` an atlas of cutouts with alpha -- both
// eyes looking at the viewer, both eyes looking to the viewer's left, and a smiling, a sad
// and an angry mouth, a mouth being one piece or several (the Hals mouths are four: lips,
// goatee, a moustache half each side). Each piece has a rect in the atlas and a placement
// rect on the base, in the base's own UV, flattened into `part_atlas`/`part_place`: the four
// eye pieces at 0..4, then every mouth piece, variant `v`'s spanning indices
// mouth_bounds[v]..mouth_bounds[v+1]. The fragment samples the base, then lays the parts
// over it: the two eye parts of the centre variant crossfaded with the left variant's by
// `eye_left`, and each mouth variant's pieces composited over one another (last on top,
// premultiplied over) before the three variants are weighted by `mouth_w`. Whole variants
// are summed premultiplied, so two variants may overlap only where their alphas partition or
// their weights sum to one, and an eye may never overlap a mouth (painting.rs tests this).
//
// THE EYES: inside each eye's ellipse (`eye[i]`, the lid edge at r = 1) the part is sampled
// at a warped UV, `uv - gaze_i * w(r)` with `w` 1 at the centre and falling to 0 at the
// lid: the iris and pupil slide toward the viewer by the whole offset, the sclera squeezes
// toward the far corner and stretches from the near one, and the lids do not move. The
// offset is in base UV and is the CPU's (painting.rs `iris_offsets`): the tangent of the
// angle from each eye to this pass's viewer, scaled, and clamped to a fraction of the eye's
// width that the warp carries without folding -- the falloff's steepest slope times the
// offset must stay under the ellipse's radius, which `iris_core` and the clamp together
// guarantee. A look further to the viewer's left than the clamp allows crossfades to the
// left-looking variant (the sheets have no right-looking one: rightward the centre variant
// warps to its clamp and stays there).
//
// UV: the loader leaves GL t=0 at the image TOP, and quad.obj's v is 1 at the top, so the
// quad's v is flipped once here and everything from the atlas is top-origin after that.
//
// Over it all a light varnish vignette and the same squared-distance fog as gltfunlit.frag
// toward `fog_color`, the walls' own fog tone, so a portrait at the far end of the hall
// fades with the wall it hangs on. The sources are paintings already: no canvas weave or
// craquelure is added.
//
// THE KEY (painting.rs, "The key in the painting"): one portrait carries a key painted in
// anamorphosis. The undistorted key lives on a virtual picture plane through the canvas
// centre, perpendicular to the line from a sweet spot `key_view` (in canvas metres, the
// canvas itself being z = 0) to that centre. For each canvas fragment the ray from the sweet
// spot through it is intersected with that plane, the hit is expressed in the plane's own
// (u, v) basis, and the key's signed distance is sampled THERE. Painted on the canvas the
// result is a long smear that only closes into a key from the sweet spot -- the classic
// construction (Holbein's skull). `key_view.w` is 0 on a portrait without a key. `key_state`
// fades the painted key out as the real one comes off the canvas (0 painted .. 1 gone) and
// `key_glint` is 1 while the player stands in the sweet spot: the paint catches the light.

uniform sampler2D base;     // the sitter, sockets blank and no mouth
uniform sampler2D parts;    // the eye and mouth cutouts, with alpha
uniform vec4 cam_pos;       // this pass's eye, world space
uniform vec4 fog_color;     // what the far end fades to; rgb used
uniform vec4 size;          // canvas width and height in metres (x, y)
uniform vec4 part_atlas[16]; // each piece's rect in `parts`, (u0, v0, u1, v1); eyes at 0..4,
                             // then the mouth pieces (painting.rs MAX_PIECES)
uniform vec4 part_place[16]; // each piece's rect on the base, the same form
uniform vec4 mouth_bounds;  // mouth variant v's pieces are indices [v]..[v+1] ([0] is 4)
uniform vec4 eye[2];        // the eye openings, left then right: centre (xy), radii (zw)
uniform vec4 gaze;          // the iris offsets in base UV: left eye xy, right eye zw
uniform float eye_left;     // 0 the centre eyes .. 1 the left-looking ones
uniform float iris_core;    // the warp's core radius, painting.rs IRIS_CORE; see `warped`
uniform vec4 mouth_w;       // the mouths' weights: smile, sad, angry (x, y, z)
uniform vec4 key_view;      // the sweet spot in canvas metres (xyz); w = 1 if there is a key
uniform float key_state;    // 0 painted .. 1 gone
uniform float key_glint;    // 1 while the player stands in the sweet spot
uniform float time;         // seconds, for the glint's sweep

in vec2 ex_uv;
in vec3 ex_world;

out vec4 fragColor;

// ── The key ──────────────────────────────────────────────────────────────────────────────────
// The silhouette's numbers, in metres, in the key's own frame (x along its length, bow at -x,
// teeth hanging toward -y): THE SAME NUMBERS AS tools/gen_key.py, which builds the 3D key
// that comes out of the canvas. Change them together.
#define KEY_LENGTH 0.09
#define KEY_BOW_R 0.019
#define KEY_BOW_HOLE 0.010
#define KEY_BOW_CX (-KEY_LENGTH * 0.5 + KEY_BOW_R)
#define KEY_SHAFT_HW 0.004
#define KEY_SHAFT_X0 (KEY_BOW_CX + KEY_BOW_R * 0.7)
#define KEY_SHAFT_X1 (KEY_LENGTH * 0.5)
// The teeth as (x0, x1, depth): a rectangle from the centreline down to y = -depth.
#define KEY_TOOTH1 vec3(0.034, 0.045, 0.013)
#define KEY_TOOTH2 vec3(0.020, 0.028, 0.010)
// Where the key sits on the picture plane, in its (u, v) metres from the canvas centre:
// across the sitter's bodice, below the neckline and above the arms, where the dress is
// dark and gold reads, and a little toward the far end of the canvas, because the near end
// is behind the frame's upright from the sweet spot. Mirrored in painting.rs
// (`KEY_ON_PLANE`), which puts the 3D key at the same spot.
#define KEY_ON_PLANE vec2(-0.010, -0.16)

float sdBox(vec2 p, vec2 half_size) {
	vec2 d = abs(p) - half_size;
	return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0);
}

float sdTooth(vec2 q, vec3 tooth) {
	vec2 half_size = vec2(0.5 * (tooth.y - tooth.x), 0.5 * tooth.z);
	return sdBox(q - vec2(0.5 * (tooth.x + tooth.y), -0.5 * tooth.z), half_size);
}

// The key's silhouette in its own frame (see the defines): a ring, a shaft, two teeth.
float sdKey(vec2 q) {
	float ring = abs(length(q - vec2(KEY_BOW_CX, 0.0)) - 0.5 * (KEY_BOW_R + KEY_BOW_HOLE))
		- 0.5 * (KEY_BOW_R - KEY_BOW_HOLE);
	float shaft = sdBox(q - vec2(0.5 * (KEY_SHAFT_X0 + KEY_SHAFT_X1), 0.0),
		vec2(0.5 * (KEY_SHAFT_X1 - KEY_SHAFT_X0), KEY_SHAFT_HW));
	return min(min(ring, shaft), min(sdTooth(q, KEY_TOOTH1), sdTooth(q, KEY_TOOTH2)));
}

// The anamorphosis (header): the canvas point `p` (z = 0) seen from `view`, carried onto the
// picture plane through the origin perpendicular to `view`, in that plane's (u, v) basis --
// u horizontal, v the nearest thing to up. Mirrored on the CPU by painting.rs `to_plane`,
// which the unit tests exercise.
vec2 anamorph(vec2 p, vec3 view) {
	vec3 n = normalize(view);
	vec3 u = normalize(cross(vec3(0.0, 1.0, 0.0), n));
	vec3 v = cross(n, u);
	vec3 P = vec3(p, 0.0);
	float t = dot(n, view) / dot(n, view - P);
	vec3 Q = view + t * (P - view);
	return vec2(dot(Q, u), dot(Q, v));
}

// Anti-aliased coverage of a distance, one pixel wide.
float fill(float d, float px) { return 1.0 - smoothstep(-px, px, d); }

// Part `i` sampled at the base UV `uv`, premultiplied: clear outside its placement rect.
// The sample is unconditional (no branch round a texture fetch, so the mip level stays
// right) with the rect's UV clamped: the atlas pads every part with clear pixels inside its
// rect, so the clamped edge is clear too, and the step masks the rest.
vec4 part(int i, vec2 uv) {
	vec4 pl = part_place[i];
	vec2 t = (uv - pl.xy) / (pl.zw - pl.xy);
	vec2 inside = step(vec2(0.0), t) * step(t, vec2(1.0));
	vec4 at = part_atlas[i];
	vec4 c = texture(parts, mix(at.xy, at.zw, clamp(t, 0.0, 1.0)));
	c.a *= inside.x * inside.y;
	return vec4(c.rgb * c.a, c.a);
}

// The base UV to sample an eye part at: inside the opening `e`, slid against the gaze
// offset `g` by the falloff (header) -- the whole offset inside `iris_core` of the radius,
// easing to none at the lid. The CPU sizes the offset's clamp from the same core so the
// warp never folds (painting.rs `IRIS_CORE`).
vec2 warped(vec2 uv, vec4 e, vec2 g) {
	float r = length((uv - e.xy) / e.zw);
	float w = 1.0 - smoothstep(iris_core, 1.0, r);
	return uv - g * w;
}

void main(void) {
	// Top-origin UV into the base (header).
	vec2 buv = vec2(ex_uv.x, 1.0 - ex_uv.y);
	vec3 col = texture(base, buv).rgb;

	// ── The parts, premultiplied and summed: eyes, left then right, each the centre
	// variant crossfaded with the left one at the warped UV; then the weighted mouths,
	// each variant's pieces composited over one another (last on top) before the weight.
	vec4 acc = vec4(0.0);
	for (int i = 0; i < 2; i++) {
		vec2 g = (i == 0) ? gaze.xy : gaze.zw;
		vec2 uv = warped(buv, eye[i], g);
		acc += mix(part(i, uv), part(2 + i, uv), eye_left);
	}
	for (int v = 0; v < 3; v++) {
		vec4 m = vec4(0.0);
		for (int i = int(mouth_bounds[v]); i < int(mouth_bounds[v + 1]); i++) {
			vec4 c = part(i, buv);
			m = c + m * (1.0 - c.a);
		}
		acc += m * mouth_w[v];
	}
	col = col * (1.0 - min(acc.a, 1.0)) + acc.rgb;

	// ── The key, in gold leaf, sampled through the anamorphosis so the smear's
	// anti-aliasing width is the distance field's own footprint per pixel, not the
	// canvas's. Gone (key_state 1) once the real key has come off.
	vec2 p = (ex_uv - 0.5) * size.xy;   // canvas metres, origin at the centre, y up
	if (key_view.w > 0.5 && key_state < 1.0) {
		vec2 q = anamorph(p, key_view.xyz) - KEY_ON_PLANE;
		float qpx = max(fwidth(q.x), fwidth(q.y)) + 1e-5;
		float dk = sdKey(q);
		float m_key = fill(dk, qpx) * (1.0 - key_state);
		// Leaf: a warm gold, darker toward the silhouette's edge as a painted outline, with
		// a light from the upper left.
		vec3 leaf = vec3(0.86, 0.68, 0.24) * (0.80 + 0.30 * smoothstep(-0.06, 0.06, q.y - q.x));
		leaf *= 1.0 - 0.45 * (1.0 - smoothstep(0.0, 0.0025, -dk));
		// In the sweet spot the leaf catches the light: a band of highlight sweeps along it.
		float sweep = 0.5 + 0.5 * sin(time * 3.0 - q.x * 90.0);
		leaf = mix(leaf, vec3(1.0, 0.96, 0.78), key_glint * 0.6 * sweep);
		col = mix(col, leaf, m_key);
	}

	// ── Old varnish: a warm cast, darkest in the corners.
	float edge = min(min(ex_uv.x, 1.0 - ex_uv.x), min(ex_uv.y, 1.0 - ex_uv.y));
	col *= vec3(1.0, 0.97, 0.90) * (0.80 + 0.20 * smoothstep(0.0, 0.22, edge));

	// ── The hall's fog, so the portrait fades with its wall.
	float d = length(ex_world - cam_pos.xyz) * 0.0195;
	float fog = 1.0 - exp(-d * d);
	col = mix(col, fog_color.rgb, fog);

	fragColor = vec4(col, 1.0);
}
