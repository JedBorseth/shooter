shader_type canvas_item;
// The HUD minimap's plan (scripts/net/minimap.gd): a window onto the map's
// top-down image (tools/build_minimaps.gd), centred on the player and turned
// so they face up, and recoloured as Black Ops 3 shades its minimap (pale
// ground, dark blocks). Past the plan's edge, thin red contour lines mark
// the ground outside the map, as Black Ops 3's minimap marks its bounds. Jammed
// (an enemy Counter-UAV), it shows static instead.

uniform sampler2D plan : source_color, filter_linear, repeat_disable;
// The player's place on the image, in UV.
uniform vec2 centre = vec2(0.5);
// How much of the image's width the window's width spans.
uniform float span = 0.5;
// The window's height over its width.
uniform float aspect = 1.0;
// Turns the image under the window, in radians.
uniform float turn = 0.0;
uniform float opacity = 1.0;
// 0 to 1: how far static has replaced the plan.
uniform float jammed = 0.0;
uniform float time = 0.0;
// One metre in UV, for the contour lines round the map's edge.
uniform float metre = 0.01;
uniform vec4 bounds_colour : source_color = vec4(0.66, 0.31, 0.24, 0.7);
// The ground's bounding box on the image, in UV.
uniform vec2 ground_min = vec2(0.0);
uniform vec2 ground_max = vec2(1.0);
uniform vec3 floor_colour : source_color = vec3(0.722, 0.769, 0.8);
uniform vec3 room_colour : source_color = vec3(0.6, 0.65, 0.69);
uniform vec3 block_colour : source_color = vec3(0.298, 0.341, 0.384);

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

void fragment() {
	vec2 p = vec2(UV.x - 0.5, (UV.y - 0.5) * aspect);
	float c = cos(turn);
	float s = sin(turn);
	vec2 uv = centre + vec2(c * p.x - s * p.y, s * p.x + c * p.y) * span;
	vec4 source = texture(plan, uv);
	// Recolour the plan the way Black Ops 3 shades its minimap: open ground
	// a pale grey-blue slab, rooms a shade darker, and solid blocks (walls,
	// roofs, tanks) dark, with a thin light edge where they meet.
	float l = dot(source.rgb, vec3(0.3, 0.59, 0.11));
	vec3 tone = floor_colour;
	tone = mix(tone, room_colour, 1.0 - smoothstep(0.12, 0.15, l));
	float block = smoothstep(0.24, 0.42, l);
	tone = mix(tone, block_colour, block);
	float edge = smoothstep(0.03, 0.12, fwidth(l)) * 0.5;
	tone = mix(tone, vec3(0.816, 0.847, 0.863), edge);
	vec4 plan_colour = vec4(tone, source.a * mix(0.74, 0.9, max(block, edge)));
	// Off the map: thin red contour lines round its edge, a metre and a half
	// apart and fading out, rounded at its corners (the distance to the
	// ground's bounding box, in metres).
	vec2 box_centre = (ground_min + ground_max) * 0.5;
	vec2 outside = max(abs(uv - box_centre) - (ground_max - ground_min) * 0.5, vec2(0.0));
	float d = length(outside) / metre;
	if (d > 0.0) {
		float fw = max(fwidth(d), 0.0001);
		float off = abs(mod(d + 0.75, 1.5) - 0.75);
		float line = 1.0 - smoothstep(0.1, 0.1 + fw * 1.2, off);
		line *= step(0.75, d) * (1.0 - smoothstep(4.5, 6.5, d));
		plan_colour = vec4(bounds_colour.rgb, bounds_colour.a * line);
	}
	// Quieten the plan towards the window's edge.
	float rim = max(abs(UV.x - 0.5), abs(UV.y - 0.5)) * 2.0;
	plan_colour.a *= opacity * mix(1.0, 0.7, smoothstep(0.75, 1.0, rim));
	if (jammed > 0.0) {
		vec2 cell = floor(UV * vec2(96.0, 96.0 * aspect)) + floor(time * 24.0) * vec2(7.0, 13.0);
		float n = hash(cell);
		float band = step(0.82, fract(UV.y * 3.0 - time * 1.7)) * 0.25;
		vec4 noise = vec4(vec3(0.55 + 0.45 * n), (0.35 + 0.45 * n + band) * opacity);
		plan_colour = mix(plan_colour, noise, jammed);
	}
	COLOR = plan_colour;
}
