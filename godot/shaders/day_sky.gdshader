// The Testing Area's sky: its sky image, with the country beyond the range's
// walls drawn into it. Rolling hills stand in rings out to 600 m, each ring
// higher than the one in front so it shows over the walls, with tree lines
// along the middle crests; the farther a ring, the more it fades into the
// haze at the horizon. Drawing them in the sky instead of as geometry costs a
// little arithmetic on the sky's own pixels and nothing else; they sit far
// enough away that the missing parallax does not show inside the range.
shader_type sky;

uniform sampler2D panorama : source_color, filter_linear, hint_default_black;
uniform float energy = 1.0;
uniform vec3 hill_color : source_color = vec3(0.52, 0.5, 0.36);
uniform vec3 tree_color : source_color = vec3(0.26, 0.31, 0.2);
// How bright sunlit country is against its albedo (the sun is strong).
uniform float hill_light = 1.5;
// The haze's density, as the map's fog (Environment.fog_density) sees it.
uniform float haze_density = 0.0024;
// Eye height over the floor, in metres.
uniform float eye = 1.7;

float ring_height(float az, float base, float amp, vec3 phase) {
	return base + amp * (0.55 * sin(3.0 * az + phase.x) + 0.3 * sin(7.0 * az + phase.y) + 0.15 * sin(13.0 * az + phase.z));
}

// Rounded crowns, broken by gaps where the tree line stops.
float trees(float az, float r, float seed) {
	float u = az * r / 6.0 + seed;
	float crown = sqrt(max(sin(fract(u) * PI), 0.0));
	float cell = floor(u);
	float h = fract(sin(cell * 12.9898 + seed) * 43758.5453);
	float stand = step(0.3, fract(sin(floor(u / 9.0) * 78.233 + seed) * 43758.5453));
	return stand * (7.0 + 6.0 * h) * (0.55 + 0.45 * crown);
}

void sky() {
	vec3 col = texture(panorama, SKY_COORDS).rgb * energy;
	vec3 dir = normalize(EYEDIR);
	if (dir.y < 0.25 && !AT_CUBEMAP_PASS) {
		float el = atan(dir.y, length(dir.xz));
		float az = atan(dir.x, dir.z);
		// The haze colour: the sky image just above the horizon.
		vec3 haze = texture(panorama, vec2(SKY_COORDS.x, 0.49)).rgb * energy;
		// Far to near, so nearer rings cover farther ones.
		vec4 rings[4] = vec4[](vec4(600.0, 62.0, 26.0, 0.0), vec4(450.0, 48.0, 22.0, 0.0),
			vec4(300.0, 34.0, 14.0, 1.0), vec4(200.0, 22.0, 9.0, 1.0));
		vec3 phases[4] = vec3[](vec3(0.3, 2.1, 4.4), vec3(1.7, 5.2, 0.8), vec3(3.9, 0.6, 2.7), vec3(5.4, 3.3, 1.2));
		for (int k = 0; k < 4; k++) {
			vec4 ring = rings[k];
			float h = ring_height(az, ring.y, ring.z, phases[k]);
			float tree = ring.w > 0.5 ? trees(az, ring.x, float(k) * 17.0) : 0.0;
			float top = atan(h + tree - eye, ring.x);
			if (el < top) {
				bool is_tree = el > atan(h - eye, ring.x);
				vec3 albedo = is_tree ? tree_color : hill_color;
				// Lower slopes a shade darker, a soft edge at the crest.
				float slope = clamp((top - el) / 0.02, 0.0, 1.0);
				vec3 land = albedo * hill_light * mix(1.0, 0.8, slope);
				float fog = 1.0 - exp(-haze_density * ring.x * 1.4);
				col = mix(land, haze, fog);
			}
		}
	}
	COLOR = col;
}
