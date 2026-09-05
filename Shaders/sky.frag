#version 150
precision highp float;

// EXT: replaces the ported gradient+sun sky (kept as sky_plain.frag.txt). The clouds come
// from two panoramas baked by Shaders/cloudbake.* (see src/ext/skybake.rs), cross-faded so
// the cloud field evolves continuously; plus a slow longitude drift. Two texture fetches and
// the original sun term -- cheap enough for every portal pass.
//
// # One bake, three skies
//
// The panorama is a DAYLIGHT sky, and that is what the daylight and storm grades use: its own
// colours, graded. The SUNSET grade wants something else entirely. It takes the panorama's
// ALPHA -- the coverage the bake wrote there (src/ext/skybake.rs) -- as a mask saying where
// the clouds are, throws the rest away, and paints its own gradient underneath and its own
// cloud colours on top, keyed to a sun of its own. So one 1536x768 bake every six seconds
// serves an overcast meadow and the sunset beyond its door at the same time.
//
// # Why the sunset is only ever seen through a doorway
//
// The intro (`src/level15.rs`) keeps its meadow under the grey: the point of the scene is that
// the warm evening is on the FAR side of the door and nowhere else. `mood` is chosen per render
// pass by where that pass's camera is (`src/ext/view.rs`), so the portal pass looking through
// the doorway grades itself as the sunset while the main pass around it stays stormy -- both in
// the same frame, on the same clouds, for the cost of one uniform.

#define LIGHT vec3(0.36, 0.80, 0.48)
#define SUN_SIZE 0.002
#define SUN_SHARPNESS 1.0
#define PI 3.14159265
// Revolutions per second of the drift. A cloud crosses the 60 degree view in ~25 s.
#define DRIFT 0.0065

// EXT: the sunset's sun. It is aimed down the line from the title screen's vantage THROUGH the
// intro's door (`ext::meadow::title_view`), so it sets into the sea that door opens on and is
// framed by the opening; and it is only 1.2 degrees up, because higher than that and the
// door's own lintel crops the disc out of the shot. Duplicated in Shaders/sea.frag exactly as
// LIGHT already is; GLSL 150 has no #include, and this file is the definition that one cites.
#define EVE_SUN vec3(-0.2929, 0.0140, -0.9560)

uniform sampler2D tex;
uniform sampler2D tex2;
uniform float blend;
uniform float time;
// -1 daylight, 0 storm, 1 sunset, 2 interior, 3 dusk (src/ext/view.rs). Colour grades of the
// same baked clouds -- except interior, which has none.
uniform float mood;
in vec3 ex_normal;
out vec4 fragColor;

// +/- half a code value of noise, added just before output. A sky is nothing but wide smooth
// gradients, which is precisely what an 8-bit framebuffer renders as visible bands. One hash
// per pixel buys the banding away for nothing.
float dither(vec2 p) {
	return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453) - 0.5;
}

// Roll the highlights off instead of letting them clip.
//
// The sky around a setting sun is meant to go past 1: that overshoot is what makes it read as
// a light source rather than as a bright colour. Clipping ruins it, and not because it loses
// detail -- because it loses it PER CHANNEL. Red saturates a few degrees from the sun, green a
// few degrees closer in, and the result is a hard-edged band that steps orange, then khaki,
// then white, which is what the sea's sky looked like through the door. This is a linear
// response up to `KNEE` and a hyperbola asymptotic to exactly 1 above it, so the channels keep
// their ratios and the gradient survives.
#define KNEE 0.80
vec3 shoulder(vec3 c) {
	vec3 over = max(c - vec3(KNEE), vec3(0.0));
	return min(c, vec3(KNEE)) + (1.0 - KNEE) * (over / (over + (1.0 - KNEE)));
}

// The sunset sky over the sea: gradient, the warmth pooled around the sun, the clouds re-lit
// from underneath, and the disc itself.
//
// `cov` is the bake's own cloud coverage, from the panorama's alpha; `lum` is how bright the
// bake made this part of the cloud, which is the light and shade the palette is painted
// through. Nothing here reads the panorama's colours.
vec3 sunset_sky(vec3 n, vec3 sun, float lum, float cov) {
	float h = n.y;

	// ── Gradient: four stops, because three cannot hold both a narrow hot band at the
	// horizon and a long violet fall to the zenith without one eating the other. The stops
	// are packed into the BOTTOM of the hemisphere on purpose -- this sky is seen through a
	// doorway, which is a few degrees of altitude and no more, and a gradient spread evenly
	// from 0 to 1 would spend its whole range on sky the opening cannot show.
	vec3 hor = vec3(1.00, 0.45, 0.17);
	vec3 low = vec3(0.82, 0.31, 0.16);
	vec3 mid = vec3(0.56, 0.24, 0.38);
	vec3 zen = vec3(0.20, 0.20, 0.50);
	vec3 base = mix(hor, low, smoothstep(-0.03, 0.035, h));
	base = mix(base, mid, smoothstep(0.02, 0.15, h));
	base = mix(base, zen, smoothstep(0.11, 0.46, h));

	// ── Where the sun is. `sd` is the direct term (a round pool of light); `az` ignores
	// altitude and asks only whether you are facing the sun's compass point, which is what
	// spreads the warmth up the sky in a wedge instead of a circle.
	//
	// Both lobes are TIGHT, and their AMPLITUDES are held under the shoulder. A doorway shows
	// about eight degrees of sky, and a gentle lobe covers all of it at nearly full strength
	// -- one flat band, which is the opposite of what a sunset looks like. Worse, the shoulder
	// (`KNEE`) runs twice, here and again in post_resolve, so anything that overshoots by much
	// has its green and blue pulled up to meet its red and the most saturated part of a real
	// sunset comes out as the least saturated part of this one. Keeping red clipped and the
	// other two under the knee is what lets CHROMA carry the gradient instead of value.
	float sd = max(dot(n, sun), 0.0);
	vec3 flat_n = vec3(n.x, 0.0, n.z);
	vec3 flat_s = normalize(vec3(sun.x, 0.0, sun.z));
	float az = max(dot(normalize(flat_n + vec3(1e-5, 0.0, 0.0)), flat_s), 0.0);
	float low_band = 1.0 - smoothstep(0.0, 0.28, h);
	base += vec3(0.95, 0.40, 0.13) * pow(sd, 130.0) * 0.50;
	base += vec3(0.40, 0.17, 0.07) * pow(az, 24.0) * low_band * 0.24;

	// ── Clouds. How much sun one catches depends on being near the sun's compass point AND
	// low enough to be lit from UNDERNEATH -- which is the whole look of a sunset cloud, and
	// the one thing the daylight bake, lit from above, cannot supply.
	float core = smoothstep(0.72, 1.00, lum);
	float caught = pow(az, 2.5) * (1.0 - smoothstep(0.02, 0.60, h));
	// The shadowed body of a sunset cloud is nearly black-violet and its lit underside is
	// nearly white-gold; that RANGE is the drama, and a palette that keeps both ends inside
	// the mid tones is what makes a procedural sunset look like a gradient with lumps in it.
	vec3 cloud = mix(vec3(0.26, 0.16, 0.30), vec3(1.45, 0.92, 0.68), core * 0.82);
	cloud = mix(cloud, vec3(1.75, 0.72, 0.30), caught * 0.80);
	// Thin the layer into the horizon haze, where the flat-layer projection stretches anyway.
	cov *= smoothstep(-0.03, 0.035, h);
	vec3 sky = mix(base, cloud, cov * 0.92);

	// ── The disc, and the bloom around it.
	sky += vec3(0.90, 0.46, 0.20) * pow(sd, 380.0) * 1.10;
	sky = mix(sky, vec3(1.80, 1.32, 0.86), smoothstep(0.99950, 0.99985, sd));

	// Everything above may have gone past 1 on purpose; this is where it comes back, with its
	// hue intact.
	return shoulder(sky);
}

void main(void) {
	vec3 n = normalize(ex_normal);
	// Direction -> equirectangular (u wraps; v=0.5 at the horizon).
	float u = atan(n.x, -n.z) / (2.0 * PI) + 0.5 + time * DRIFT;
	float v = 0.5 + asin(clamp(n.y, -1.0, 1.0)) / PI;
	// RGB is the baked daylight sky; A is the cloud coverage under it (src/ext/skybake.rs).
	vec4 pano = mix(texture(tex, vec2(u, v)), texture(tex2, vec2(u, v)), blend);
	vec3 sky = pano.rgb;

	// The sun is drawn at runtime so it never drifts with the clouds (original formula).
	float s = dot(n, LIGHT) - 1.0 + SUN_SIZE;
	float sun = min(exp(s * SUN_SHARPNESS / SUN_SIZE), 1.0);

	float lum = dot(sky, vec3(0.3, 0.59, 0.11));

	if (mood > 2.5) {
		// DUSK: the intro's meadow, and it is OVERCAST. The sunset is through the door and
		// nowhere else; this is the weather you are standing in while you look at it, and its
		// whole job is to be the dull thing the doorway is bright against.
		//
		// So: grey. Not a stylised grey -- a real one, which is not flat. An overcast sky is
		// brightest just above the horizon (you are looking through the least cloud there, and
		// what is behind it is the lit underside of the deck) and darkest at the zenith, where
		// you are looking up through the whole thickness of it. That vertical range is most of
		// what makes cloud read as cloud rather than as a grey wall, and it is the same reason
		// the storm grade darkens overhead.
		vec3 eve = normalize(EVE_SUN);
		float h = n.y;
		vec3 flat_n = vec3(n.x, 0.0, n.z);
		float az = max(dot(normalize(flat_n + vec3(1e-5, 0.0, 0.0)),
		                   normalize(vec3(eve.x, 0.0, eve.z))), 0.0);

		vec3 hor = vec3(0.560, 0.575, 0.590);
		vec3 low = vec3(0.505, 0.520, 0.545);
		vec3 mid = vec3(0.245, 0.265, 0.300);
		vec3 zen = vec3(0.135, 0.150, 0.180);
		// The stops are pitched for the WHOLE hemisphere, not for the wedge the title lens
		// happens to see. A 36 degree lens pitched a few degrees down shows about 13 degrees
		// of sky, and the previous stops spent their entire range inside it -- so the visible
		// band ran from the horizon colour to the zenith colour and the sky read as a steep
		// vertical ramp instead of as a ceiling.
		vec3 base = mix(hor, low, smoothstep(0.00, 0.28, h));
		base = mix(base, mid, smoothstep(0.22, 0.60, h));
		base = mix(base, zen, smoothstep(0.50, 0.95, h));
		// The one warm thing in it: the sun is below the horizon on its own bearing, and a
		// hand's breadth of the cloud base over there is still catching it. Kept small and
		// kept LOW -- past this it stops being weather and starts being a second sunset.
		float low_band = 1.0 - smoothstep(0.0, 0.16, h);
		base += vec3(0.115, 0.058, 0.022) * pow(az, 8.0) * low_band;

		// ── The deck ──────────────────────────────────────────────────────────────────
		// Two things were wrong here, and both are about what the word OVERCAST means.
		//
		// First, coverage. The bake is fair-weather cumulus -- about one okta, scattered
		// puffs with sky between them. Overcast is eight oktas by definition: an unbroken
		// sheet, whose structure is variation in optical THICKNESS rather than holes. So the
		// coverage is floored, and the bake's alpha keeps only the last fifteen per cent of
		// the say -- enough to be the thick and thin of the sheet, not enough to tear it.
		// Floored HERE rather than in the bake: `pano.a` is shared with the portal's sunset,
		// where extending coverage below the bake's horizon would lay a bar of cloud across
		// the sea.
		//
		// Second, radiance. A cloud is not a grey object hung in front of the sky -- it is a
		// translucent layer, and what reaches the eye is the light behind it times how much
		// gets through. So the deck MULTIPLIES the gradient it sits in rather than replacing
		// it with a flat pair of greys, which is what makes it keep the vertical falloff and
		// stop reading as a wall.
		float y = max(n.y, 0.0);
		// What the bake would have drawn here with NO cloud: its own gradient, luma-weighted
		// (cloudbake.frag). Needed because `lum` reads the bake's COMPOSITED colour, so
		// wherever coverage is partial it is mostly backdrop -- and a thick shadowed flank
		// and clear horizon haze come out at the same luminance, which inverts the deck's
		// light and shade. Dividing the backdrop back out recovers the cloud's own radiance,
		// which is what a camera pointed at a cloud records.
		float bg = mix(0.782, 0.438, pow(y, 0.55));
		bg = mix(bg, 0.849, pow(1.0 - y, 9.0));
		float cl = clamp((lum - bg * (1.0 - pano.a)) / max(pano.a, 0.35), 0.0, 1.0);
		// Where the bake has little or no cloud, `cl` is a difference of two nearly equal
		// numbers divided by a floor -- noise, and it lands near zero, which would paint the
		// bake's CLEAR sky as the deck's darkest part and put a hard edge along every cloud
		// boundary. Those blobs were the whole reason the sky read as cut paper. So the thin
		// places are given average thickness instead, and only where there is real cloud does
		// its own radiance get a say.
		cl = mix(0.55, cl, smoothstep(0.05, 0.45, pano.a));
		// A wide ramp on purpose. `cl` is a noise field divided by another noise field, so it
		// is far grainier than the cloud it describes; a tight smoothstep on it cuts the deck
		// into hard-edged blobs, which is a stencil and not a sky.
		float core = smoothstep(0.32, 1.00, cl);
		vec3 cloud = base * mix(0.88, 1.16, core);
		cloud += vec3(0.085, 0.042, 0.016) * pow(az, 6.0) * (1.0 - smoothstep(0.0, 0.22, h));
		float cov = mix(0.85, 1.0, pano.a) * smoothstep(-0.05, 0.02, h);
		sky = mix(base, cloud, cov);
		sky = shoulder(sky);
		sun = 0.0;
	} else if (mood > 1.5) {
		// INTERIOR: the Backrooms' outside is no sky at all. Near-black with the faintest warm
		// cast at the horizon -- the wall maps' own colour leaking into the dark -- so a gap in
		// the single-sided walls reads as the building going on into darkness. No sun, no
		// clouds: the panorama taps above are simply not used.
		float h = clamp(n.y * 2.0, 0.0, 1.0);
		sky = mix(vec3(0.030, 0.024, 0.016), vec3(0.008, 0.007, 0.006), h);
		sun = 0.0;
	} else if (mood > 0.5) {
		// SUNSET: the sea through the intro's door, with the sun still on the water.
		sky = sunset_sky(n, normalize(EVE_SUN), lum, pano.a);
		sun = 0.0;
	} else if (mood > -0.5) {
		// STORM: desaturated, dark, heavier overhead.
		vec3 grey = vec3(lum);
		vec3 tint = vec3(0.36, 0.40, 0.46);
		sky = mix(sky, grey, 0.85) * tint * 1.1;
		sky *= mix(0.9, 0.55, clamp(n.y * 1.8, 0.0, 1.0));  // darker overhead
		sun = 0.0;
	}

	sky = max(sky, vec3(sun)) + dither(gl_FragCoord.xy) * (1.0 / 255.0);
	fragColor = vec4(sky, 1.0);
}
