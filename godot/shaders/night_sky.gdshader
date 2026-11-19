// A clear night sky drawn in code: a dark gradient with a glow of distant
// town light along the horizon, stars, thin moonlit cloud, the moon, and the
// country round the arena against it. The moon stands on the bearing the
// scene's first DirectionalLight3D (the moonlight) comes from, so shadows fall
// away from it, but lower in the sky (`moon_elevation`), where a player
// looking over the walls can see it; drawing it lower moves no light.
//
// The country is a strip rendered in Blender (tools/build_horizon.py --night):
// tree belts, a town with lit windows and sodium street lamps, a factory, a
// water tower, a spire, and a line of pylons, as dark shapes against the sky.
// A second image holds each pixel's distance, so far shapes fade into the
// haze. Two texture reads on the sky's pixels near the horizon.
//
// It reads only the shared noise texture and never uses TIME, so the sky's radiance (which
// lights reflections, such as the moon in the puddles) is computed once.
// Per pixel it costs a little arithmetic on the pixels where the sky shows.
shader_type sky;

uniform vec3 zenith : source_color = vec3(0.17, 0.21, 0.33);
uniform vec3 horizon : source_color = vec3(0.32, 0.37, 0.46);
// Distant town light, low on the horizon toward `glow_dir`.
uniform vec3 town_glow : source_color = vec3(0.3, 0.22, 0.16);
// Toward the town in the skyline strip; a unit vector along the ground.
uniform vec3 glow_dir = vec3(0.2588, 0.0, -0.9659);
uniform vec3 moon_tint : source_color = vec3(0.92, 0.95, 1.0);
// Angular radius of the moon, in radians (drawn larger than life, as games
// do, so it reads on screen).
uniform float moon_radius = 0.03;
// The moon's height in the sky, in radians; 0 draws it where the light is.
uniform float moon_elevation = 0.42;
uniform float moon_energy = 3.5;
uniform float halo_energy = 0.18;
uniform float star_energy = 1.4;
uniform float cloud_amount : hint_range(0.0, 1.0) = 0.35;
// Off on the Low preset (scripts/map_render.gd): no cloud, stars, or skyline.
uniform bool detailed = true;
// The skyline strip: land premultiplied by coverage (RGB) and coverage (A),
// and its distance as a fraction of `depth_range` metres. The strip's middle
// column faces the moon's bearing. Keep its size in step with
// build_horizon.py (LAT_MIN, and HEIGHT * 360 / WIDTH degrees).
uniform sampler2D skyline : source_color, filter_linear_mipmap, hint_default_transparent;
uniform sampler2D skyline_depth : filter_linear_mipmap, hint_default_white;
uniform float depth_range = 3000.0;
uniform float strip_min = -0.0610865;
uniform float strip_span = 0.3436117;
uniform float land_light = 1.0;
uniform float haze_density = 0.0025;
// The cloud's second, finer noise read: High only (scripts/map_render.gd), so
// the skyline costs Medium nothing over the sky it replaced.
uniform bool fine_clouds = true;
// The world shader's tiling value noise (GraphicsQuality sets it).
global uniform sampler2D world_noise : filter_linear_mipmap, repeat_enable;

float hash3(vec3 p) {
	p = fract(p * 0.3183099 + 0.1);
	p *= 17.0;
	return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}

float noise3(vec3 p) {
	vec3 i = floor(p);
	vec3 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	return mix(
		mix(mix(hash3(i), hash3(i + vec3(1, 0, 0)), f.x),
			mix(hash3(i + vec3(0, 1, 0)), hash3(i + vec3(1, 1, 0)), f.x), f.y),
		mix(mix(hash3(i + vec3(0, 0, 1)), hash3(i + vec3(1, 0, 1)), f.x),
			mix(hash3(i + vec3(0, 1, 1)), hash3(i + vec3(1, 1, 1)), f.x), f.y),
		f.z);
}

void sky() {
	vec3 dir = normalize(EYEDIR);
	vec3 light_dir = normalize(LIGHT0_DIRECTION);
	vec3 to_moon = light_dir;
	if (moon_elevation > 0.0) {
		vec2 bearing = normalize(light_dir.xz + 1e-5);
		to_moon = vec3(bearing.x * cos(moon_elevation), sin(moon_elevation), bearing.y * cos(moon_elevation));
	}
	// The country, in front of everything else. Where it covers the sky
	// completely, the sky behind it (cloud, stars, moon) is not worked out.
	vec4 land = vec4(0.0);
	vec3 land_col = vec3(0.0);
	// Only the strip's band, and not on Low: the rest of the sky pays a
	// compare and nothing more.
	if (detailed && dir.y < sin(strip_min + strip_span) && !AT_CUBEMAP_PASS) {
		float v = (asin(dir.y) - strip_min) / strip_span;
		float az = atan(dir.x, dir.z) - atan(light_dir.x, light_dir.z);
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
		vec2 uv = vec2(u, 1.0 - max(v, 0.002));
		land = textureGrad(skyline, uv, dx, dy);
		float dist = textureGrad(skyline_depth, uv, dx, dy).r * depth_range;
		float fog = 1.0 - exp(-haze_density * dist * 1.4);
		land_col = mix(land.rgb * land_light, horizon * land.a, fog);
	}
	vec3 col = vec3(0.0);
	if (land.a < 0.996) {
		float cos_moon = dot(dir, to_moon);
		// Angle from the moon, cheaply: exact enough near it, where it matters.
		float angle = sqrt(max(2.0 - 2.0 * cos_moon, 0.0));
		float clouds = 0.0;
		{
			float up = clamp(dir.y, 0.0, 1.0);
			// The gradient, and a lighter band of haze at the horizon.
			col = mix(horizon, zenith, pow(up, 0.45));
			float glow = max(dot(normalize(vec3(dir.x, 0.0, dir.z) + 1e-5), glow_dir), 0.0);
			glow = glow * glow * glow;
			col += town_glow * glow * exp(-up * 9.0);
			// Below the horizon (seen only in reflections).
			col = dir.y < 0.0 ? horizon * 0.6 : col;
			// Thin cloud, lit from the moon's side, thicker near the horizon.
			if (detailed && dir.y > 0.0) {
				// Two reads of the shared noise texture, projected on a cloud
				// layer over the yard.
				vec2 q = dir.xz / max(dir.y + 0.15, 0.15) * 0.25;
				float n = texture(world_noise, q).r;
				if (fine_clouds) {
					n = n * 0.65 + texture(world_noise, q * 2.7 + 0.37).r * 0.35;
				}
				clouds = smoothstep(1.0 - cloud_amount, 1.0, n) * smoothstep(0.0, 0.25, dir.y);
				vec3 lit = horizon * 1.6 + moon_tint * 0.05 * exp(-angle * 2.5);
				col = mix(col, lit, clouds * 0.8);
			}
		}

		if (detailed && !AT_CUBEMAP_PASS) {
			// Stars: one candidate per cell of a grid on the sphere; few cells
			// hold one. Fainter near the horizon, behind cloud, and near the moon.
			vec3 p = dir * 180.0;
			vec3 cell = floor(p);
			float h = hash3(cell);
			if (h > 0.99 && dir.y > 0.0) {
				vec3 centre = cell + 0.5 + (vec3(hash3(cell + 3.1), hash3(cell + 5.7), hash3(cell + 9.2)) - 0.5) * 0.6;
				float d = length(p - centre);
				float star = smoothstep(0.35, 0.0, d) * (h - 0.99) / 0.01;
				star *= smoothstep(0.02, 0.3, dir.y) * (1.0 - clouds) * smoothstep(0.1, 0.4, angle);
				col += vec3(0.85, 0.9, 1.0) * star * star_energy;
			}
		}

		// The moon: a disc with a soft edge, darker toward its limb, with darker
		// seas, and a halo round it.
		if (cos_moon > 0.9) {
			float r = angle / moon_radius;
			if (r < 1.0) {
				vec3 side = normalize(cross(to_moon, vec3(0.0, 1.0, 0.0)));
				vec3 upv = cross(side, to_moon);
				vec2 face = vec2(dot(dir, side), dot(dir, upv)) / moon_radius;
				float seas = noise3(vec3(face * 2.2, 1.7)) * 0.6 + noise3(vec3(face * 5.0, 4.3)) * 0.4;
				float limb = sqrt(max(1.0 - r * r, 0.0));
				float disc = smoothstep(1.0, 0.92, r);
				vec3 moon = moon_tint * moon_energy * (0.6 + 0.4 * limb) * (1.0 - 0.35 * smoothstep(0.45, 0.7, seas));
				col = mix(col, moon, disc * (1.0 - clouds * 0.6));
			}
			col += moon_tint * halo_energy * exp(-angle * 18.0) * (1.0 - clouds * 0.5);
		}
	}
	COLOR = col * (1.0 - land.a) + land_col;
}
