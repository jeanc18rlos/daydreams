#version 150

// EXT: dust rising out of the intro's doorway (src/ext/doorlight.rs). Each point is a SEED, not
// a position: everything about where this mote is now is computed here from `time`, so there is
// nothing to simulate on the CPU and nothing to reset when the scene reloads.
//
// in_pos = (u, v, seed): where in the opening the mote is born, and the number every other
// decision about it is drawn from.
//
// # Why it RISES rather than flies at you
//
// The first version pushed every mote along the sun's direction. That direction is aimed
// through the doorway at the title camera (`Shaders/sky.frag`, EVE_SUN), so the motes travelled
// almost exactly toward the eye -- and a particle moving along your line of sight does not
// appear to move at all. It grows a little and that is all. The plume read as a scatter of
// specks hanging around the door rather than as anything leaving it.
//
// What reads is motion ACROSS the frame, and the one direction guaranteed to be across it from
// any vantage is up. So the motion is mostly a rise, with a modest push out of the opening and
// a lateral wander that widens with age: warm air off a sunlit floor lifting the dust in it,
// which is also what is physically happening in a doorway with a sunset behind it.

uniform mat4 vp;
uniform mat4 basis;      // the door's yaw: +x runs across the opening, +z out through it
uniform vec4 mouth;      // centre of the opening, world space
uniform vec4 drift;      // x = rise, y = push out of the doorway, z = wander, w = lifetime
uniform vec4 gap;        // x = half width, y = half height, z = unused, w = door openness
uniform float time;
uniform float viewport_h;

in vec3 in_pos;

out float ex_fade;

void main(void) {
	float seed = in_pos.z;
	// Age, as a fraction of a life, offset per mote so they are not all born together.
	float life = fract(seed * 7.3197 + time / drift.w);

	vec3 across = (basis * vec4(1.0, 0.0, 0.0, 0.0)).xyz;
	vec3 out_ = (basis * vec4(0.0, 0.0, 1.0, 0.0)).xyz;
	vec3 up = vec3(0.0, 1.0, 0.0);

	// Born IN the opening, and biased to the lower half of it: the air that is moving is the
	// air over the sunlit threshold, so that is where dust gets picked up. `v * v` does the
	// bias; the 0.86 keeps them off the jambs, where a speck reads as dirt on the paint.
	float v = in_pos.y * in_pos.y;
	vec3 p = mouth.xyz
	       + across * (in_pos.x * 2.0 - 1.0) * gap.x * 0.86
	       + up * (v * 2.0 - 1.0) * gap.y * 0.86;

	// Rise, slowing as it goes -- dust lifted by warm air decelerates as the air cools and
	// spreads. `1 - (1-t)^2` is that ease, and it also crowds the motes near the doorway,
	// which is what a plume looks like and what says where they came from.
	float ease = 1.0 - (1.0 - life) * (1.0 - life);
	p += up * ease * drift.x;
	// And a push out through the gap, small: the point is that they LEAVE, not that they are
	// fired.
	p += out_ * life * drift.y;

	// Wander: three slow circles at different rates, widening with age. Without it the plume
	// is a fountain; with it, it is dust.
	float w1 = seed * 37.0, w2 = seed * 61.0, w3 = seed * 91.0;
	float amp = drift.z * (0.12 + 0.88 * life);
	p += across * sin(time * 0.55 + w1) * amp;
	p += up * sin(time * 0.41 + w2) * amp * 0.35;
	p += out_ * cos(time * 0.47 + w3) * amp * 0.8;

	vec4 clip = vp * vec4(p, 1.0);
	gl_Position = clip;

	// A mote is a fixed WORLD size, so its pixel size is that over the distance to it. The
	// clamp keeps a near one from becoming a plate and a far one from dropping below a pixel,
	// where point rasterisation would make it flicker in and out between frames.
	//
	// It also SHRINKS with age. Not because dust shrinks, but because the ones near the door
	// are the ones lit hardest by it, and a bigger, brighter speck there against a smaller,
	// fainter one further up is the whole of what says the plume has a source.
	float radius = mix(0.010, 0.022, fract(seed * 131.0)) * (1.0 - 0.45 * life);
	gl_PointSize = clamp(radius * viewport_h / max(clip.w, 0.05), 1.5, 20.0);

	// Brightest as it crosses the threshold and dimmer as it climbs out of the light, which is
	// the other half of saying where it came from. Times the door's own openness.
	ex_fade = smoothstep(0.0, 0.07, life) * (1.0 - smoothstep(0.30, 0.85, life)) * gap.w;
}
