// An explosion's core: camera-facing particles that play a flipbook of a
// fireball flashing out, burning, and rolling up into dark smoke, simulated
// and rendered in Blender (`assets/textures/effects/explosion_{a,b}.png`,
// `tools/build_flipbooks.py`: a Mantaflow fire, 36 cells of 448 pixels). The
// flipbook is a six-way lightmap (`sixway.gdshaderinc`): its smoke is lit by
// every light in the scene per pixel from the side the light comes from, so
// the cloud takes the sun on its sunward side, falls into shade on the other
// and never glows at night; the b atlas's alpha holds the flame, which glows
// from deep orange at its thin edges to yellow-white in the core, at a
// bounded HDR brightness so the bloom gives it a short halo without blowing
// out the picture. `white_heat` burns the flame's core white for the blast's
// opening frames. It fades out where it meets the ground or a wall, so it has
// no hard edge there.
shader_type spatial;
render_mode blend_mix, cull_disabled, depth_draw_never, shadows_disabled;

#include "res://shaders/sixway.gdshaderinc"

uniform sampler2D flipbook : source_color, filter_linear_mipmap, repeat_disable;
uniform sampler2D flipbook_b : source_color, filter_linear_mipmap, repeat_disable;
uniform int frames_across = 6;
uniform int frames_down = 6;
/// The first frame played, and the last (-1: the flipbook's last).
uniform int first_frame = 0;
uniform int last_frame = -1;
/// How brightly the flame glows.
uniform float fire_energy = 3.2;
/// The fire's colour from its dim edges to its hottest core.
uniform vec3 fire_edge : source_color = vec3(0.9, 0.32, 0.06);
uniform vec3 fire_core : source_color = vec3(1.0, 0.72, 0.36);
/// The smoke's colour (times the flipbook's grey).
uniform vec3 smoke_tint : source_color = vec3(0.85, 0.8, 0.74);
/// The flipbook's light scaled up to the smoke's brightness (the atlas is
/// normalised to its brightest texel).
uniform float light_gain = 1.9;
/// Over this distance (metres) in front of the ground or a wall the cloud
/// fades, so it has no hard edge where it meets them. Reading the depth
/// buffer costs a copy of it (about 0.1 ms on Medium), only in frames with
/// an explosion on screen.
uniform float soft_distance = 0.8;
uniform sampler2D depth_texture : hint_depth_texture, filter_nearest;
/// Animation progress (0 to 1) over which the flame dies away (the
/// simulation cools on its own; this shortens it).
uniform vec2 cooling = vec2(0.12, 0.5);
/// Animation progress over which the particle fades out (a frag's core
/// gives way to the dirt it throws up instead of hanging as a ball).
uniform vec2 fade_out = vec2(1.0, 1.0);
/// How far the flame's hottest parts burn toward white (the opening flash).
uniform float white_heat = 0.0;

varying float progress;
varying vec2 cell_x;
varying vec4 lit_a;
varying vec4 lit_b;
varying float lit_mean;

void vertex() {
	// Face the camera, turned by the particle's angle, keeping its scale.
	mat4 world = mat4(
		normalize(INV_VIEW_MATRIX[0]),
		normalize(INV_VIEW_MATRIX[1]),
		normalize(INV_VIEW_MATRIX[2]),
		MODEL_MATRIX[3]
	);
	float a = INSTANCE_CUSTOM.x;
	world = world * mat4(
		vec4(cos(a), -sin(a), 0.0, 0.0),
		vec4(sin(a), cos(a), 0.0, 0.0),
		vec4(0.0, 0.0, 1.0, 0.0),
		vec4(0.0, 0.0, 0.0, 1.0)
	);
	cell_x = vec2(cos(a), -sin(a));
	MODELVIEW_MATRIX = VIEW_MATRIX * world * mat4(
		vec4(length(MODEL_MATRIX[0].xyz), 0.0, 0.0, 0.0),
		vec4(0.0, length(MODEL_MATRIX[1].xyz), 0.0, 0.0),
		vec4(0.0, 0.0, length(MODEL_MATRIX[2].xyz), 0.0),
		vec4(0.0, 0.0, 0.0, 1.0)
	);
	MODELVIEW_NORMAL_MATRIX = mat3(MODELVIEW_MATRIX);
	progress = clamp(INSTANCE_CUSTOM.z, 0.0, 1.0);
	float total = float(frames_across * frames_down);
	float first = float(first_frame);
	float last = last_frame < 0 ? total - 1.0 : float(last_frame);
	float frame = min(first + floor(progress * (last + 1.0 - first)), last);
	UV = (UV + vec2(mod(frame, float(frames_across)), floor(frame / float(frames_across))))
		/ vec2(float(frames_across), float(frames_down));
}

void fragment() {
	vec4 a = texture(flipbook, UV);
	vec4 b = texture(flipbook_b, UV);
	float heat = 1.0 - smoothstep(cooling.x, cooling.y, progress);
	float flame = sixway_glow(b) * heat;
	lit_a = a;
	lit_b = b;
	lit_mean = max(sixway_ambient(a, b), 0.02);
	// Where it burns, the gas is light, not a surface: its smoke shading
	// gives way to the glow.
	ALBEDO = smoke_tint * lit_mean * light_gain * (1.0 - smoothstep(0.05, 0.4, flame));
	vec3 fire = mix(fire_edge, fire_core, smoothstep(0.1, 0.8, flame));
	fire = mix(fire, vec3(1.0, 0.95, 0.88), smoothstep(0.4, 1.0, flame) * white_heat);
	EMISSION = fire * flame * fire_energy;
	ROUGHNESS = 1.0;
	SPECULAR = 0.0;
	float depth = textureLod(depth_texture, SCREEN_UV, 0.0).r;
	vec4 behind = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, depth, 1.0);
	behind.xyz /= behind.w;
	float soft = clamp((VERTEX.z - behind.z) / soft_distance, 0.0, 1.0);
	float gone = fade_out.x < 1.0 ? smoothstep(fade_out.x, fade_out.y, progress) : 0.0;
	ALPHA = a.a * COLOR.a * soft * (1.0 - gone);
}

void light() {
	vec3 l = vec3(dot(LIGHT.xy, cell_x), dot(LIGHT.xy, vec2(-cell_x.y, cell_x.x)), LIGHT.z);
	float lit = sixway_light(lit_a, lit_b, normalize(l)) / lit_mean;
	DIFFUSE_LIGHT += lit * ATTENUATION * LIGHT_COLOR / PI;
}
