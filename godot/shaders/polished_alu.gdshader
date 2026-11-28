shader_type spatial;
// Polished aluminium: Breakwater's road tanker barrels (tools/breakwater_models.py
// `tank_alu`). A mirror finish reads by what it reflects: the bright sky
// above the horizon and the darker, sun-warmed ground below it, split by a
// hard line that bends round the barrel's curve. The map has no reflection
// probes, so Godot reflects the sky in every direction; this shader darkens
// the metal where the reflection points below the horizon and adds the
// ground's colour there as emission. Faint polishing streaks run along the
// barrel, and road film dulls its lower third. Two texture reads on the
// barrel's pixels only.
render_mode cull_back, specular_schlick_ggx;

uniform vec3 metal_color : source_color = vec3(0.91, 0.92, 0.93);
// The ground the barrel reflects (yard asphalt and concrete in sun) and how
// bright that reflection is: about the sunlit ground's own brightness.
uniform vec3 ground_color : source_color = vec3(0.5, 0.47, 0.42);
uniform float ground_energy = 0.9;
// The road film near the bottom: its colour and the band it covers (metres
// above the ground the barrel stands on, measured from the bottom up).
uniform vec3 film_color : source_color = vec3(0.36, 0.33, 0.29);
uniform float film_top = 2.0;
uniform float film_bottom = 1.35;
global uniform sampler2D world_noise : filter_linear_mipmap, repeat_enable;

varying vec3 world_pos;
varying vec3 world_norm;

void vertex() {
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	world_norm = normalize(MODEL_NORMAL_MATRIX * NORMAL);
}

void fragment() {
	vec3 n = normalize(world_norm);
	// The barrel lies level, so its axis is square to the normal and to up.
	vec3 axis = cross(n, vec3(0.0, 1.0, 0.0));
	float axis_len = length(axis);
	axis = axis_len > 0.08 ? axis / axis_len : vec3(1.0, 0.0, 0.0);
	vec3 across = cross(vec3(0.0, 1.0, 0.0), axis);
	float along = dot(world_pos, axis);
	float round_ = world_pos.y + dot(world_pos, across);
	// Streaks: noise stretched along the axis, fine across it.
	float streak = texture(world_noise, vec2(along * 0.02, round_ * 0.9)).r;
	float smudge = texture(world_noise, world_pos.xz * 0.37 + world_pos.y * 0.21).r;
	float film = (1.0 - smoothstep(film_bottom, film_top, world_pos.y)) * (0.55 + 0.45 * smudge);

	vec3 view_w = normalize(CAMERA_POSITION_WORLD - world_pos);
	vec3 r = reflect(-view_w, n);
	// Below the horizon: the ground, with a slightly soft line.
	float ground = smoothstep(0.015, -0.05, r.y);
	float rough = 0.2 + 0.1 * streak + 0.05 * smudge + 0.3 * film;
	// Rougher metal blurs the line.
	ground = mix(ground, smoothstep(0.2, -0.25, r.y), clamp(rough * 1.6 - 0.15, 0.0, 1.0));

	vec3 metal = metal_color * (0.92 + 0.08 * streak);
	metal = mix(metal, film_color, film * 0.6);
	ALBEDO = mix(metal, metal * 0.28, ground);
	METALLIC = 1.0;
	ROUGHNESS = rough;
	// The reflected ground, through Fresnel's rise at glancing angles.
	float fresnel = 0.75 + 0.25 * pow(1.0 - clamp(dot(n, view_w), 0.0, 1.0), 4.0);
	EMISSION = ground_color * ground_energy * ground * fresnel * (1.0 - film * 0.5) * metal;
}
