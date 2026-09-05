#version 150
precision highp float;

// EXT: a cheap sunset sea. Two scrolled noise taps fake the wave field, a Fresnel term blends
// the water colour toward the sky's horizon colour at grazing angles, and a long specular
// streak toward the sun gives the glittering band. No reflection pass -- the "reflected sky"
// is the same gradient Shaders/sky.frag evaluates, written out again here.
//
// # The one thing this has to get right
//
// Through the intro's door you see the sea and the sky above it at once, joined along a
// horizon line a few hundred units out. Any disagreement between the two shows up there as a
// visible seam, so the horizon colour, the sun's direction and the sun's own colour below are
// the same numbers as in Shaders/sky.frag -- and the sun the water reflects is the disc that
// file draws, sitting exactly where the reflection says it should.

uniform sampler2D tex;       // grass_noise.bmp, reused purely as a noise source
uniform vec4 cam_pos;
uniform float time;
uniform float mood;
in vec3 ex_world;
out vec4 fragColor;

// EXT: the evening sun. Defined in Shaders/sky.frag; copied here as LIGHT is elsewhere.
#define EVE_SUN vec3(-0.2929, 0.0140, -0.9560)

// Identical to Shaders/sky.frag's: the sea meets that sky along the horizon, and a highlight
// rolled off differently on the two sides of the line shows up as a step along it.
#define KNEE 0.80
vec3 shoulder(vec3 c) {
	vec3 over = max(c - vec3(KNEE), vec3(0.0));
	return min(c, vec3(KNEE)) + (1.0 - KNEE) * (over / (over + (1.0 - KNEE)));
}

void main(void) {
	vec2 uv = ex_world.xz;
	// Wave field: three octaves drifting in different directions. The third is small and fast
	// and exists only to break the specular into separate glints -- two octaves gave one
	// continuous smear where the reference has a path made of individual sparks.
	float w1 = texture(tex, uv * 0.045 + vec2(time * 0.020, time * 0.013)).g;
	float w2 = texture(tex, uv * 0.120 - vec2(time * 0.031, time * 0.009)).b;
	float w3 = texture(tex, uv * 0.420 + vec2(time * 0.055, -time * 0.042)).r;
	float wave = (w1 - 0.5) * 0.8 + (w2 - 0.5) * 0.5;

	// A normal tilted by the wave slope (cheap finite difference via the taps). The chop from
	// the fast octave is folded in at a fraction of the weight, so it ruffles the surface
	// without turning the whole plane matte.
	vec3 n = normalize(vec3(wave * 0.9 + (w3 - 0.5) * 0.55, 1.0,
	                        (w2 - w1) * 0.6 + (w3 - 0.5) * 0.45));

	vec3 V = normalize(cam_pos.xyz - ex_world);
	float fresnel = pow(1.0 - max(dot(n, V), 0.0), 3.0);

	bool sunset = mood > 0.5 && mood < 1.5;
	vec3 deep    = sunset ? vec3(0.080, 0.042, 0.080) : vec3(0.10, 0.22, 0.34);
	vec3 shallow = sunset ? vec3(0.360, 0.160, 0.130) : vec3(0.22, 0.42, 0.52);
	// Matches `hor` in Shaders/sky.frag's evening gradient at `late` = 0, so the water and the
	// sky arrive at the same colour from either side of the horizon line.
	vec3 horizon = sunset ? vec3(1.12, 0.52, 0.22) : vec3(0.74, 0.82, 0.90);
	vec3 water = mix(deep, shallow, clamp(wave + 0.5, 0.0, 1.0));
	// 0.62, not 0.85: a doorway looks at water almost edge-on, so the Fresnel term is near 1
	// across the whole visible sheet and any more than this leaves no water at all -- just the
	// sky's colour lying flat where the sea should be.
	vec3 col = mix(water, horizon, fresnel * 0.62);

	vec3 sunDir = normalize(EVE_SUN);
	// The path of light. A sun this low reflects as a COLUMN rather than a spot -- the water's
	// slopes spread its image along the line between it and you -- so the specular is taken
	// against a normal whose sideways tilt is squashed, which stretches the highlight
	// towards the camera exactly the way the real thing stretches.
	vec3 nCol = normalize(vec3(n.x * 0.35, n.y, n.z));
	vec3 H = normalize(V + sunDir);
	float spec = pow(max(dot(nCol, H), 0.0), 220.0);
	float glint = pow(max(dot(n, H), 0.0), 900.0);   // individual sparks on the wave tops
	col += vec3(1.35, 0.92, 0.58) * spec * 2.2;
	col += vec3(1.40, 1.15, 0.85) * glint * 2.6 * smoothstep(0.45, 0.95, w3);

	// The sky's own aureole, REFLECTED. Water cannot return more of the sky than the sky puts
	// out, and what it returns is scaled by Fresnel -- so this is the same colour, the same
	// exponent and a fraction of the amplitude that Shaders/sky.frag uses for the same wedge,
	// times the reflectance the horizon colour above is already mixed by. At `pow(az, 6.0)`
	// it was none of those things: every pixel of sea the doorway shows lies within a few
	// degrees of the sun's bearing, so the term ran 0.98..1.00 across the whole sheet and was
	// simply a black-level lift -- on top of the same wedge the distance haze below already
	// applies, counting it twice.
	float az = max(dot(normalize(vec3(-V.x, 0.0, -V.z)), normalize(vec3(sunDir.x, 0.0, sunDir.z))), 0.0);
	col += vec3(0.44, 0.21, 0.09) * pow(az, 14.0) * 0.40 * (fresnel * 0.62);

	// Distance haze toward the horizon colour, warmed further along the sun's own bearing so
	// the far water does not go flat.
	float dist = length(ex_world - cam_pos.xyz);
	float fog = (1.0 - exp(-dist * 0.0075)) * 0.92;
	col = mix(col, horizon * (1.0 + 0.16 * pow(az, 4.0)), fog);
	fragColor = vec4(shoulder(col), 1.0);
}
