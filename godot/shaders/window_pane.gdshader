// A window pane with a room behind it (interior mapping): the view ray is
// intersected with a box behind the glass, so every window on Slamcity's
// facades shows a furnished room that moves with parallax, for the cost of
// one quad. tools/build_slamcity_city.py writes each pane's size (metres / 10)
// into the vertex colour's red and green and a random seed into its blue.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

#include "res://shaders/rooms.gdshaderinc"

// Brightness of the rooms, and the share with their lamp on.
uniform float room_light = 0.3;
uniform float lit_rooms = 0.2;
uniform float room_depth = 4.0;
// Share of windows with blinds drawn part way down.
uniform float blinds = 0.5;
// Height of the window's bottom edge above the room's floor.
uniform float sill = 0.9;

varying vec3 world_position;
varying vec3 world_normal;
varying vec4 pane;

void vertex() {
	world_position = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
	pane = COLOR;
}

void fragment() {
	vec3 n = normalize(world_normal);
	// Room axes: x to the viewer's right, y up, z into the building.
	vec3 right = normalize(cross(vec3(0.0, 1.0, 0.0), n));
	vec2 size = max(pane.rg * 10.0, vec2(0.2));
	float seed = pane.b * 97.0;
	vec3 room = vec3(max(size.x * 1.7, 2.6), 2.8, room_depth);
	vec3 p = vec3(UV.x * size.x + (room.x - size.x) * 0.5, sill + (1.0 - UV.y) * size.y, 0.0);
	p.y = min(p.y, room.y - 0.01);
	vec3 v = normalize(world_position - CAMERA_POSITION_WORLD);
	vec3 d = vec3(dot(v, right), v.y, dot(v, -n));
	float lit;
	vec3 c = room_colour(p, d, seed, room, room_light, lit_rooms, lit);
	// Blinds or curtains drawn part way down some windows.
	float drawn = hash(vec2(seed, 4.4)) < blinds ? hash(vec2(seed, 8.1)) * 0.8 : 0.0;
	float slats = 0.85 + 0.15 * smoothstep(0.3, 0.5, abs(fract(UV.y * size.y * 12.0) - 0.5));
	vec3 blind_col = mix(vec3(0.78, 0.74, 0.66), vec3(0.62, 0.64, 0.6), hash(vec2(seed, 6.6)));
	c = mix(c, blind_col * slats * room_light * mix(0.8, 2.4, lit), step(UV.y, drawn));
	// The wall's thickness shades the top and the sides of the opening.
	float reveal = smoothstep(0.0, 0.15, UV.y * size.y) * smoothstep(0.0, 0.08, min(UV.x, 1.0 - UV.x) * size.x);
	c *= mix(0.45, 1.0, reveal);
	// Dirty glass: smooth with smudges, so the sky's reflection breaks up.
	float smudge = value_noise(world_position.xz * 3.0 + world_position.y * 2.7);
	ALBEDO = vec3(0.012);
	ROUGHNESS = mix(0.04, 0.22, smudge * smudge);
	METALLIC = 0.0;
	SPECULAR = 0.5;
	EMISSION = c;
}
