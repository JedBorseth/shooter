// Shared by world.gdshader (skyline windows) and window_pane.gdshader:
// hashing, value noise, and the fake rooms seen through windows.

// Sine-free hash (Dave Hoskins), cheaper than fract(sin()) on most GPUs.
float hash(vec2 p) {
	vec3 p3 = fract(vec3(p.xyx) * 0.1031);
	p3 += dot(p3, p3.yzx + 33.33);
	return fract((p3.x + p3.y) * p3.z);
}

float value_noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	return mix(
		mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x),
		mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x),
		f.y
	);
}

// A fake room behind one window: the colour seen along the view ray, lit by the
// room itself. `p` is where the ray enters, in metres inside the room's front
// face (x right, y up, z into the building); `d` is the ray in the same axes;
// `size` is the room (width, height, depth). `light` scales its brightness and
// `lit_share` is the chance that its lamp is on.
vec3 room_colour(vec3 p, vec3 d, float seed, vec3 size, float light, float lit_share, out float lit) {
	// Keep the ray off exactly zero on any axis.
	d = mix(d, vec3(1e-4), lessThan(abs(d), vec3(1e-4)));
	vec3 t3 = (step(vec3(0.0), d) * size - p) / d;
	float t = min(t3.x, min(t3.y, t3.z));
	vec3 h = p + d * t;
	vec3 wall = mix(vec3(0.62, 0.58, 0.5), vec3(0.75, 0.73, 0.7), hash(vec2(seed, 3.1)));
	wall *= mix(vec3(1.0), vec3(0.75, 0.85, 1.0), step(0.75, hash(vec2(seed, 7.7))));
	vec3 c;
	if (t == t3.y) {
		// Floor (carpet or boards) or ceiling.
		c = d.y < 0.0 ? mix(vec3(0.22, 0.17, 0.13), vec3(0.32, 0.3, 0.28), hash(vec2(seed, 1.3)))
				: vec3(0.82, 0.8, 0.76);
	} else if (t == t3.x) {
		c = wall * 0.8;
	} else {
		c = wall;
		// Something on the back wall: a door, a shelf, or a picture.
		vec2 q = h.xy / size.xy;
		float kind = hash(vec2(seed, 9.2));
		vec2 lo = kind < 0.4 ? vec2(0.55, 0.0) : vec2(0.25, 0.4);
		vec2 hi = kind < 0.4 ? vec2(0.8, 0.68) : vec2(0.6, 0.62);
		if (all(greaterThan(q, lo)) && all(lessThan(q, hi))) {
			c = kind < 0.4 ? vec3(0.35, 0.26, 0.18) : vec3(0.18, 0.22, 0.26);
		}
	}
	// Darker toward the back and into the corners, as a room lit from its window.
	c *= mix(1.0, 0.45, h.z / size.z);
	lit = step(1.0 - lit_share, hash(vec2(seed, 5.5)));
	c *= light * mix(0.7, 1.3, hash(vec2(seed, 2.4)));
	c = mix(c, c * vec3(2.6, 2.1, 1.5) + vec3(0.06, 0.04, 0.02), lit);
	return c;
}
