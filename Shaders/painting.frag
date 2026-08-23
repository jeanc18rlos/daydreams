#version 150
precision highp float;

// EXT: a procedural oil portrait whose eyes follow the viewer, for src/ext/painting.rs.
//
// There is no portrait image in the asset set, so the sitter is drawn here from signed-distance
// shapes: a vignetted ground, shoulders and a collar, a neck, hair behind and over an oval
// head, two almond eyes with whites, irises and pupils, brows, a nose line and a mouth. `seed`
// picks the sitter -- ground, skin, hair, iris and collar colours, and the head's width -- so
// a row of them reads as a row of different people. Over the lot go the things that make it a
// painting rather than a clip-art face: a low-frequency brush mottle, the canvas weave and a
// faint craquelure, the last two fading out where their period falls under a couple of pixels
// so they never alias into noise from across the hall.
//
// The eyes: `viewer_local` is the point the sitter looks at, in the canvas's own metres (x
// right, y up, z out of the canvas toward the room), already chosen by the CPU from this
// pass's eye -- so a portrait seen through a portal looks at the portal camera. Each iris is
// displaced toward it by the tangent of the angle from that eye, clamped so it stays inside the
// white; both eyes converge on the same point, so the nearer eye turns a little further. The
// whites never move.
//
// `expression` is the unobserved twist (see painting.rs): 0 is the neutral sitter, 1 the
// changed one -- brows lowered and knitted, the mouth gone flat and thin, the eyes narrowed,
// and the gaze no longer following but fixed straight out of the canvas. It is NOT the
// weather `mood` every other shader takes; the portrait is lit flat, like the walls' bake.
//
// Lighting is flat plus the same squared-distance fog as gltfunlit.frag toward `fog_color`,
// which the caller sets to the walls' own fog tone, so a painting at the far end of the hall
// fades with the wall it hangs on.

uniform vec4 cam_pos;       // this pass's eye, world space
uniform vec4 fog_color;     // what the far end fades to; rgb used
uniform vec4 viewer_local;  // the gaze target in canvas metres; see above
uniform vec4 size;          // canvas width and height in metres (x, y)
uniform float seed;         // which sitter
uniform float expression;   // 0 neutral .. 1 changed

in vec2 ex_uv;
in vec3 ex_world;

out vec4 fragColor;

// ── Gaze ─────────────────────────────────────────────────────────────────────────────────────
// Metres of iris travel per unit tangent of the viewing angle, and the furthest the iris may
// go: the whites are 0.040 wide and the iris 0.017, so 0.016 keeps a sliver of white beyond it.
#define GAZE_K 0.014
#define GAZE_LIMIT 0.016
// The viewer's distance off the canvas is taken as at least this, so the tangent is bounded:
// a viewer level with the canvas (or behind it) gets the iris pinned at GAZE_LIMIT toward
// their side rather than a divide by zero.
#define GAZE_MIN_Z 0.05

// ── Hashes and noise ─────────────────────────────────────────────────────────────────────────
float hash1(float n) { return fract(sin(n * 12.9898) * 43758.5453); }
float hash21(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
vec2 hash22(vec2 p) {
	p = vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)));
	return fract(sin(p) * 43758.5453);
}

float vnoise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	float a = hash21(i);
	float b = hash21(i + vec2(1.0, 0.0));
	float c = hash21(i + vec2(0.0, 1.0));
	float d = hash21(i + vec2(1.0, 1.0));
	return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

float fbm(vec2 p) {
	return 0.5 * vnoise(p) + 0.25 * vnoise(p * 2.03 + 7.1) + 0.125 * vnoise(p * 4.07 + 3.3);
}

// Distance between the nearest and second-nearest cell seeds: zero along cell borders, which
// is where old varnish cracks.
float cracks(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	float d1 = 8.0;
	float d2 = 8.0;
	for (int y = -1; y <= 1; y++) {
		for (int x = -1; x <= 1; x++) {
			vec2 g = vec2(float(x), float(y));
			vec2 r = g + hash22(i + g) - f;
			float d = dot(r, r);
			if (d < d1) { d2 = d1; d1 = d; } else if (d < d2) { d2 = d; }
		}
	}
	return sqrt(d2) - sqrt(d1);
}

// ── Shapes: signed distances, negative inside ────────────────────────────────────────────────
float sdEllipse(vec2 q, vec2 r) { return (length(q / r) - 1.0) * min(r.x, r.y); }
float sdSegment(vec2 p, vec2 a, vec2 b) {
	vec2 pa = p - a;
	vec2 ba = b - a;
	float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
	return length(pa - ba * h);
}

// Anti-aliased coverage of a distance, one pixel wide.
float fill(float d, float px) { return 1.0 - smoothstep(-px, px, d); }

// ── Palettes ─────────────────────────────────────────────────────────────────────────────────
const vec3 GROUND[4] = vec3[4](
	vec3(0.30, 0.24, 0.14),   // umber
	vec3(0.14, 0.18, 0.22),   // slate blue
	vec3(0.26, 0.11, 0.10),   // burgundy
	vec3(0.13, 0.19, 0.14)    // bottle green
);
const vec3 SKIN[3] = vec3[3](
	vec3(0.86, 0.70, 0.58),
	vec3(0.72, 0.54, 0.40),
	vec3(0.46, 0.31, 0.22)
);
const vec3 HAIR[4] = vec3[4](
	vec3(0.10, 0.08, 0.06),   // black
	vec3(0.32, 0.20, 0.10),   // brown
	vec3(0.48, 0.22, 0.10),   // auburn
	vec3(0.62, 0.60, 0.56)    // grey
);
const vec3 IRIS[4] = vec3[4](
	vec3(0.30, 0.18, 0.08),   // brown
	vec3(0.30, 0.42, 0.52),   // blue
	vec3(0.28, 0.38, 0.22),   // green
	vec3(0.40, 0.32, 0.14)    // hazel
);
const vec3 COLLAR[3] = vec3[3](
	vec3(0.88, 0.86, 0.80),   // white linen
	vec3(0.80, 0.72, 0.52),   // cream
	vec3(0.36, 0.10, 0.10)    // dark red
);

// One eye, its brow, and the lashes, in a frame whose +x points AWAY from the nose, so the same
// code draws both sides: `q` is the fragment relative to the eye's centre, `gaze` the iris
// offset in that frame, `hw` the head width factor. Layers the eye over `col`.
vec3 eye(vec3 col, vec2 q, vec2 gaze, vec3 iris_col, vec3 brow_col, float hw, float expr, float px) {
	vec2 r = vec2(0.040 * hw, 0.021 * (1.0 - 0.15 * expr));
	float d_white = sdEllipse(q, r);
	float m_white = fill(d_white, px);

	// The white, shaded under the upper lid.
	vec3 white = vec3(0.86, 0.83, 0.76) * (1.0 - 0.35 * smoothstep(0.2, 1.0, q.y / r.y));
	// Iris and pupil, clipped to the white. The iris darkens toward its rim (the limbal ring).
	vec2 c = q - gaze;
	float ri = 0.017;
	float dc = length(c);
	vec3 iris = iris_col * (1.25 - 0.8 * smoothstep(0.3, 1.0, dc / ri));
	float m_iris = fill(dc - ri, px);
	float m_pupil = fill(dc - 0.0075, px);
	// A catchlight, fixed on the iris so the eye reads as wet whichever way it looks.
	float m_light = fill(length(c - vec2(-0.006, 0.006)) - 0.0035, px);
	vec3 e = mix(white, iris, m_iris);
	e = mix(e, vec3(0.03, 0.02, 0.02), m_pupil);
	e = mix(e, vec3(0.95, 0.94, 0.90), m_light * 0.9);
	col = mix(col, e, m_white);

	// Lashes: a dark line along the upper rim, thinning toward the inner corner.
	float lash = fill(abs(d_white) - 0.0022 * (0.6 + 0.4 * smoothstep(-r.x, r.x, q.x)), px)
		* smoothstep(-0.004, 0.004, q.y);
	col = mix(col, vec3(0.08, 0.05, 0.04), lash * 0.85);

	// The brow: an arc from the inner end to the outer, higher at the outer end when at ease.
	// Changed: the whole brow drops and the inner end drops further -- knitted.
	float drop = 0.018 * expr;
	vec2 inner = vec2(-0.045 * hw, 0.044 - drop - 0.012 * expr);
	vec2 outer = vec2(0.050 * hw, 0.052 - drop);
	float along = clamp((q.x - inner.x) / (outer.x - inner.x), 0.0, 1.0);
	float thick = 0.0075 * (1.0 - 0.55 * along);
	float m_brow = fill(sdSegment(q, inner, outer) - thick, px);
	col = mix(col, brow_col, m_brow * 0.92);
	return col;
}

void main(void) {
	// Canvas metres, origin at the centre: y spans +-h/2, x spans +-w/2.
	vec2 p = (ex_uv - 0.5) * size.xy;
	float px = fwidth(p.y) * 1.0 + 1e-5;

	// ── The sitter: which of the palettes, and how broad a face.
	// The indices are stepped so that no two of the first eight seeds share both a ground and
	// a hair: the second four take the grounds in order and the hairs one step on.
	float s = floor(seed + 0.5);
	vec3 ground = GROUND[int(mod(s, 4.0))];
	vec3 skin = SKIN[int(mod(s * 2.0 + 1.0, 3.0))];
	vec3 hair = HAIR[int(mod(s + floor(s / 4.0), 4.0))];
	vec3 iris_col = IRIS[int(mod(s * 3.0 + 1.0, 4.0))];
	vec3 collar = COLLAR[int(mod(s + 1.0, 3.0))];
	float hw = 0.90 + 0.20 * hash1(s + 7.3);
	float expr = clamp(expression, 0.0, 1.0);

	// ── Ground: a vignette that is brightest behind the head, with a broad brushy mottle.
	float halo = 1.0 - smoothstep(0.08, 0.70, length((p - vec2(0.0, 0.12)) * vec2(1.0, 0.85)));
	vec3 col = ground * (0.45 + 0.75 * halo) * (0.88 + 0.24 * fbm(p * 9.0 + s * 3.1));

	// ── Shoulders and coat, from below the frame, shaded from the left.
	vec2 head_c = vec2(0.0, 0.08);
	vec2 head_r = vec2(0.150 * hw, 0.200);
	float d_coat = sdEllipse(p - vec2(0.0, -0.74), vec2(0.48, 0.50));
	vec3 coat = vec3(0.11, 0.09, 0.08) * (0.7 + 0.5 * smoothstep(0.35, -0.35, p.x));
	col = mix(col, coat, fill(d_coat, px));
	// The collar: a V opening at the throat.
	float d_collar = max(abs(p.x) - (0.045 + 1.1 * (-0.22 - p.y)), max(p.y + 0.22, -0.36 - p.y));
	col = mix(col, collar * (0.8 + 0.3 * smoothstep(0.1, -0.1, p.x)), fill(d_collar, px));

	// ── Neck, in the chin's shadow.
	float d_neck = max(abs(p.x) - 0.078 * hw, max(p.y - 0.0, -0.30 - p.y));
	col = mix(col, skin * 0.66, fill(d_neck, px));

	// ── Hair behind the head: a larger oval, streaked, cut off at a length the sitter chose --
	// from above the ears to the shoulders.
	float hair_end = head_c.y - head_r.y * mix(0.15, 1.15, hash1(s + 3.9));
	float d_hair = max(sdEllipse(p - vec2(0.0, 0.12), head_r * vec2(1.25, 1.16)), hair_end - p.y);
	float streak = 0.78 + 0.44 * vnoise(vec2(p.x * 70.0, p.y * 9.0) + s);
	col = mix(col, hair * streak, fill(d_hair, px));

	// ── The head: lit from the upper left, a little colour in the cheeks.
	float d_head = sdEllipse(p - head_c, head_r);
	float shade = 0.82 + 0.26 * smoothstep(1.0, -0.7, (p.x + 0.25 * (p.y - head_c.y)) / head_r.x);
	float blush = 0.35 * (1.0 - smoothstep(0.0, 0.07, length((p - vec2(0.0, 0.02)) * vec2(0.75, 1.0) - vec2(0.0, 0.0))))
		* smoothstep(0.03, 0.08, abs(p.x));
	vec3 face = skin * shade;
	face = mix(face, face * vec3(1.06, 0.86, 0.84), blush);
	col = mix(col, face, fill(d_head, px));

	// ── Hair over the head: everything in the hair oval above a curved hairline, whose height
	// and how far it comes down at the temples are the sitter's too.
	float hairline = head_c.y + head_r.y * (mix(0.50, 0.72, hash1(s + 1.7)) - mix(0.15, 0.45, hash1(s + 5.1)) * pow(p.x / head_r.x, 2.0));
	float d_fringe = max(d_hair, hairline - p.y);
	// The forehead just under it is in its shadow.
	col *= 1.0 - 0.22 * fill(d_head, px) * (1.0 - smoothstep(0.0, 0.035, hairline - p.y)) * step(p.y, hairline);
	col = mix(col, hair * streak * 0.95, fill(d_fringe, px));

	// ── Nose: the bridge as a fine line, and a soft shadow under the tip.
	float d_bridge = sdSegment(p, vec2(-0.003, 0.095), vec2(0.011 * hw, 0.028)) - 0.0022;
	col = mix(col, skin * 0.55, fill(d_bridge, px) * 0.6);
	float d_tip = sdEllipse(p - vec2(0.003 * hw, 0.018), vec2(0.020 * hw, 0.008));
	col = mix(col, skin * 0.62, fill(d_tip, px) * 0.55);

	// ── Mouth: a lip line that smiles at the ends when at ease and flattens, thinner, when
	// changed.
	float mw = 0.046 * hw;
	float mx = clamp(p.x, -mw, mw);
	float curve = mix(0.011, -0.003, expr);
	vec2 on_lip = vec2(mx, -0.045 + curve * (mx * mx) / (mw * mw));
	float lip_th = mix(0.0062, 0.0038, expr) * (1.0 - 0.45 * (mx * mx) / (mw * mw));
	float m_mouth = fill(length(p - on_lip) - lip_th, px);
	col = mix(col, vec3(0.46, 0.21, 0.19), m_mouth * 0.9);

	// ── Eyes. The gaze: the tangent of the angle from each eye to the viewer, scaled and
	// clamped; straight ahead when changed.
	vec3 v = viewer_local.xyz;
	float vz = max(v.z, GAZE_MIN_Z);
	vec3 brow = hair * 0.7;
	for (int side = 0; side < 2; side++) {
		float sgn = (side == 0) ? -1.0 : 1.0;   // -1 the sitter's right (screen left), +1 left
		vec2 e = vec2(sgn * 0.062 * hw, head_c.y + 0.12 * head_r.y);
		vec2 g = (v.xy - e) / vz * GAZE_K;
		float gl = length(g);
		if (gl > GAZE_LIMIT) {
			g *= GAZE_LIMIT / gl;
		}
		g *= 1.0 - expr;
		// Into the eye's own frame: +x away from the nose.
		vec2 q = vec2(sgn * (p.x - e.x), p.y - e.y);
		vec2 gq = vec2(sgn * g.x, g.y);
		col = eye(col, q, gq, iris_col, brow, hw, expr, px);
	}

	// ── Paint, canvas and varnish. Brush mottle everywhere; weave and craquelure fade out
	// where a period falls under a couple of pixels, so at distance they leave a clean tone
	// rather than noise.
	col *= 0.93 + 0.14 * fbm(p * 70.0 + s * 11.0);
	float weave_f = 160.0;
	float weave_px = fwidth(p.x * weave_f);
	float weave_fade = 1.0 - smoothstep(0.25, 0.6, weave_px);
	float weave = sin(p.x * weave_f * 6.2832) * sin(p.y * weave_f * 6.2832);
	col *= 1.0 + 0.035 * weave * weave_fade;
	float crack_f = 28.0;
	float crack_fade = 1.0 - smoothstep(0.06, 0.2, fwidth(p.x * crack_f));
	float crack = 1.0 - smoothstep(0.0, 0.035, cracks(p * crack_f + s * 5.0));
	col *= 1.0 - 0.08 * crack * crack_fade;
	// Old varnish: a warm cast, darkest in the corners.
	float edge = min(min(ex_uv.x, 1.0 - ex_uv.x), min(ex_uv.y, 1.0 - ex_uv.y));
	col *= vec3(1.0, 0.96, 0.88) * (0.78 + 0.22 * smoothstep(0.0, 0.22, edge));

	// ── The hall's fog, so the portrait fades with its wall.
	float d = length(ex_world - cam_pos.xyz) * 0.0195;
	float fog = 1.0 - exp(-d * d);
	col = mix(col, fog_color.rgb, fog);

	fragColor = vec4(col, 1.0);
}
