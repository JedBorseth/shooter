// The Testing Area's sky: its sky image, with the country beyond the range's
// walls laid round the horizon. The country is a strip rendered in Blender
// (tools/build_horizon.py): fields, hedgerows, woods, and farms on hills that
// rise toward a far ridge, all from real tree meshes lit by the same sun. A
// second image holds each pixel's distance, so the land fades into the
// map's haze as far as it really stands. Drawing it in the sky costs two
// texture reads on the sky's own pixels near the horizon and nothing else;
// it sits far enough away (110 m and more) that the missing parallax does
// not show inside the range.
shader_type sky;

uniform sampler2D panorama : source_color, filter_linear, hint_default_black;
uniform float energy = 1.0;
// The strip: land premultiplied by coverage (RGB) and coverage (A), and the
// land's distance as a fraction of `depth_range` metres.
uniform sampler2D horizon : source_color, filter_linear_mipmap, hint_default_transparent;
uniform sampler2D horizon_depth : filter_linear_mipmap, hint_default_white;
uniform float depth_range = 3000.0;
// The strip's lowest elevation and its height, in radians (build_horizon.py:
// LAT_MIN and HEIGHT * 360 / WIDTH degrees).
uniform float strip_min = -0.0610865;
uniform float strip_span = 0.3436117;
// How bright the rendered land is against the sky (the render's sun is
// weaker than the map's).
uniform float land_light = 1.3;
// The Environment's sky_rotation.y: the view direction turns with the sky,
// the sun's direction does not.
uniform float sky_turn = 0.0;
// The haze's density, as the map's fog (Environment.fog_density) sees it.
uniform float haze_density = 0.0024;

void sky() {
	vec3 col = texture(panorama, SKY_COORDS).rgb * energy;
	vec3 dir = normalize(EYEDIR);
	float el = asin(clamp(dir.y, -1.0, 1.0));
	float v = (el - strip_min) / strip_span;
	if (v < 1.0 && !AT_CUBEMAP_PASS) {
		// The strip's middle column faces the sun, and it runs clockwise
		// (seen from above) from left to right.
		vec3 sun = LIGHT0_DIRECTION;
		float az = atan(dir.x, dir.z) - atan(sun.x, sun.z) + sky_turn;
		float u = fract(0.5 - az / TAU);
		// Mip selection from a version of u that has no jump at the seam.
		float u2 = fract(u + 0.5);
		vec2 dx = vec2(dFdx(u), dFdx(v));
		vec2 dy = vec2(dFdy(u), dFdy(v));
		vec2 dx2 = vec2(dFdx(u2), dx.y);
		vec2 dy2 = vec2(dFdy(u2), dy.y);
		if (abs(dx2.x) + abs(dy2.x) < abs(dx.x) + abs(dy.x)) {
			dx = dx2;
			dy = dy2;
		}
		// Below the strip is near country; repeat its bottom row.
		vec2 uv = vec2(u, 1.0 - max(v, 0.002));
		vec4 land = textureGrad(horizon, uv, dx, dy);
		float dist = textureGrad(horizon_depth, uv, dx, dy).r * depth_range;
		// The haze colour: the sky image just above the horizon.
		vec3 haze = texture(panorama, vec2(SKY_COORDS.x, 0.49)).rgb * energy;
		float fog = 1.0 - exp(-haze_density * dist * 1.4);
		col = col * (1.0 - land.a) + mix(land.rgb * land_light, haze * land.a, fog);
	}
	COLOR = col;
}
