// A day sky with a rendered horizon: the map's sky image, with the country
// beyond its walls laid round the horizon. The country is a strip rendered in
// Blender (tools/build_horizon.py): the Testing Area's fields, hedgerows,
// woods, and farms, or Breakwater's harbour (port, ships, far shores), all
// real meshes lit by the same sun. A second image holds each pixel's
// distance, so the land fades into haze as far as it really stands. Drawing
// it in the sky costs two texture reads on the sky's own pixels near the
// horizon and nothing else; it sits far enough away (110 m and more) that the
// missing parallax does not show inside the map. The Testing Area sets it in
// its scene; Breakwater gets it from scripts/looks.gd.
shader_type sky;

uniform sampler2D panorama : source_color, filter_linear, hint_default_black;
// The same sky image read through its mipmaps, for the haze colour: a sky
// whose horizon row carries stitching streaks (Breakwater's) would otherwise
// paint them down over the land. Unset, the haze is read from `panorama`.
uniform sampler2D haze_sky : source_color, filter_linear_mipmap, hint_default_black;
uniform bool blur_haze = false;
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
// Fallback sun azimuth atan(sun.x, sun.z) in radians when no DirectionalLight3D
// is visible (e.g. a map whose live sun is High-only).
uniform float sun_azimuth = 0.0;
// The haze's density, as the map's fog (Environment.fog_density) sees it,
// and the distance in metres before haze begins (0 = fog from the camera).
uniform float haze_density = 0.0024;
uniform float haze_start = 0.0;
// Open water below the horizon, for a map whose strip leaves its sea clear
// (Breakwater): lit the way shaders/sea.gdshader lights the map's own water
// (a dark body that lightens at grazing angles, a Fresnel reflection of the
// same sky, softened by the swell, and the sun's glitter), so the two meet
// without a seam where the map's water plane ends.
uniform bool far_sea = false;
uniform float sea_level = -2.2;
uniform vec3 sea_deep : source_color = vec3(0.03, 0.065, 0.06);
uniform vec3 sea_grazing : source_color = vec3(0.07, 0.11, 0.1);
// How far the swell blurs the reflected sky, as a mip level of `haze_sky`.
uniform float sea_blur = 3.0;
uniform float sea_glint = 1.0;
// The haze over the water, as the map's own fog density.
uniform float sea_haze_density = 0.0026;

void sky() {
	vec3 col = texture(panorama, SKY_COORDS).rgb * energy;
	vec3 dir = normalize(EYEDIR);
	float el = asin(clamp(dir.y, -1.0, 1.0));
	float v = (el - strip_min) / strip_span;
	// Stop just shy of v = 1.0 (top row is alpha 0) so linear/mipmap filtering
	// never wraps across V=0 into the solid bottom row.
	if (v < 0.99 && !AT_CUBEMAP_PASS) {
		// The strip's middle column faces the sun, and it runs clockwise
		// (seen from above) from left to right.
		float sun_az = LIGHT0_ENABLED ? atan(LIGHT0_DIRECTION.x, LIGHT0_DIRECTION.z) : sun_azimuth;
		float az = atan(dir.x, dir.z) - sun_az + sky_turn;
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
		// Below the strip is near country; repeat a texel-safe bottom row and
		// zero vertical derivatives so mipmaps never bleed from the top row.
		float v_clamped = max(v, 0.012);
		if (v < 0.012) {
			dx.y = 0.0;
			dy.y = 0.0;
		}
		vec2 uv = vec2(u, 1.0 - v_clamped);
		vec4 land = textureGrad(horizon, uv, dx, dy);
		float dist = textureGrad(horizon_depth, uv, dx, dy).r * depth_range;
		// The haze colour: the sky image just above the horizon.
		vec3 haze = blur_haze
				? textureLod(haze_sky, vec2(SKY_COORDS.x, 0.475), 5.0).rgb * energy
				: texture(panorama, vec2(SKY_COORDS.x, 0.49)).rgb * energy;
		if (far_sea && dir.y < 0.0) {
			float down = -dir.y;
			float sea_dist = max(POSITION.y - sea_level, 0.3) / max(down, 0.0005);
			// Waves tilt the facets, so the reflection never quite reaches a
			// mirror's at grazing angles.
			float fresnel = 0.02 + 0.62 * pow(1.0 - down, 5.0);
			vec3 sky_seen = textureLod(haze_sky, vec2(SKY_COORDS.x, 1.0 - SKY_COORDS.y), sea_blur).rgb;
			vec3 sky_light = textureLod(haze_sky, vec2(SKY_COORDS.x, 0.2), 8.0).rgb;
			vec3 sea = mix(sea_grazing, sea_deep, down) * sky_light * energy;
			if (LIGHT0_ENABLED) {
				vec3 mirrored = vec3(dir.x, down, dir.z);
				float sun_up = max(LIGHT0_DIRECTION.y, 0.0);
				sea += mix(sea_grazing, sea_deep, down) * LIGHT0_COLOR * LIGHT0_ENERGY * sun_up * 0.3;
				// The swell spreads the sun's reflection into a wide glare on
				// the water toward it, with a brighter core.
				float toward = max(dot(mirrored, LIGHT0_DIRECTION), 0.0);
				float glitter = pow(toward, 3.0) * 0.9 + pow(toward, 160.0) * 0.6;
				sea += LIGHT0_COLOR * LIGHT0_ENERGY * glitter * sea_glint;
			}
			sea = mix(sea, sky_seen * energy, fresnel);
			col = mix(sea, haze, 1.0 - exp(-sea_haze_density * sea_dist));
		}
		float fog = 1.0 - exp(-haze_density * max(dist - haze_start, 0.0) * 1.4);
		col = col * (1.0 - land.a) + mix(land.rgb * land_light, haze * land.a, fog);
	}
	COLOR = col;
}
