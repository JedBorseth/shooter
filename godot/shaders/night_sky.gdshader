// A clear night sky drawn in code: a dark gradient with a glow of distant
// town light along the horizon, stars, thin moonlit cloud, and the moon. The
// moon sits wherever the scene's first DirectionalLight3D (the moonlight)
// comes from, so it always matches the shadows and the baked light.
//
// It reads only the shared noise texture and never uses TIME, so the sky's radiance (which
// lights reflections, such as the moon in the puddles) is computed once.
// Per pixel it costs a little arithmetic on the pixels where the sky shows.
shader_type sky;

uniform vec3 zenith : source_color = vec3(0.17, 0.21, 0.33);
uniform vec3 horizon : source_color = vec3(0.32, 0.37, 0.46);
// Distant town light, low on the horizon toward `glow_dir`.
uniform vec3 town_glow : source_color = vec3(0.3, 0.22, 0.16);
uniform vec3 glow_dir = vec3(-0.7, 0.0, 0.7);
uniform vec3 moon_tint : source_color = vec3(0.92, 0.95, 1.0);
// Angular radius of the moon, in radians (drawn larger than life, as games
// do, so it reads on screen).
uniform float moon_radius = 0.03;
uniform float moon_energy = 5.0;
uniform float halo_energy = 0.18;
uniform float star_energy = 1.4;
uniform float cloud_amount : hint_range(0.0, 1.0) = 0.35;
// Off on the Low preset (scripts/map_render.gd): no cloud or stars.
uniform bool detailed = true;
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
	vec3 to_moon = normalize(LIGHT0_DIRECTION);
	float cos_moon = dot(dir, to_moon);
	// Angle from the moon, cheaply: exact enough near it, where it matters.
	float angle = sqrt(max(2.0 - 2.0 * cos_moon, 0.0));
	vec3 col;
	float clouds = 0.0;
	{
		float up = clamp(dir.y, 0.0, 1.0);
		// The gradient, and a lighter band of haze at the horizon.
		col = mix(horizon, zenith, pow(up, 0.45));
		float glow = pow(max(dot(normalize(vec3(dir.x, 0.0, dir.z) + 1e-5), normalize(glow_dir)), 0.0), 3.0);
		col += town_glow * glow * exp(-up * 9.0);
		// Below the horizon (seen only in reflections).
		col = dir.y < 0.0 ? horizon * 0.6 : col;
		// Thin cloud, lit from the moon's side, thicker near the horizon.
		if (detailed && dir.y > 0.0) {
			// Two reads of the shared noise texture, projected on a cloud
			// layer over the yard.
			vec2 q = dir.xz / max(dir.y + 0.15, 0.15) * 0.25;
			float n = texture(world_noise, q).r * 0.65 + texture(world_noise, q * 2.7 + 0.37).r * 0.35;
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
	COLOR = col;
}
