shader_type canvas_item;
// The HUD minimap's plan (scripts/net/minimap.gd): a window onto the map's
// top-down image (tools/build_minimaps.gd), centred on the player and turned
// so they face up. Jammed (an enemy Counter-UAV), it shows static instead.

uniform sampler2D plan : source_color, filter_linear, repeat_disable;
// The player's place on the image, in UV.
uniform vec2 centre = vec2(0.5);
// How much of the image's width the window spans.
uniform float span = 0.5;
// Turns the image under the window, in radians.
uniform float turn = 0.0;
uniform float opacity = 0.9;
// 0 to 1: how far static has replaced the plan.
uniform float jammed = 0.0;
uniform float time = 0.0;

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

void fragment() {
	vec2 p = UV - 0.5;
	float c = cos(turn);
	float s = sin(turn);
	vec2 uv = centre + vec2(c * p.x - s * p.y, s * p.x + c * p.y) * span;
	vec4 plan_colour = texture(plan, uv);
	if (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0) {
		plan_colour.a = 0.0;
	}
	// Quieten the plan towards the window's edge.
	float edge = max(abs(p.x), abs(p.y)) * 2.0;
	plan_colour.a *= opacity * mix(1.0, 0.55, smoothstep(0.7, 1.0, edge));
	if (jammed > 0.0) {
		vec2 cell = floor(UV * 96.0) + floor(time * 24.0) * vec2(7.0, 13.0);
		float n = hash(cell);
		float band = step(0.82, fract(UV.y * 3.0 - time * 1.7)) * 0.25;
		vec4 noise = vec4(vec3(0.55 + 0.45 * n), (0.35 + 0.45 * n + band) * opacity);
		plan_colour = mix(plan_colour, noise, jammed);
	}
	COLOR = plan_colour;
}
