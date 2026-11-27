shader_type canvas_item;
// The HUD minimap's plan (scripts/net/minimap.gd): a window onto the map's
// top-down image (tools/build_minimaps.gd), centred on the player and turned
// so they face up. Past the plan's edge, thin red contour lines mark the
// ground outside the map, as Black Ops 3's minimap marks its bounds. Jammed
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
uniform float opacity = 0.9;
// 0 to 1: how far static has replaced the plan.
uniform float jammed = 0.0;
uniform float time = 0.0;
// One metre in UV, for the contour lines round the map's edge.
uniform float metre = 0.01;
uniform vec4 bounds_colour : source_color = vec4(0.82, 0.3, 0.24, 0.55);

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

void fragment() {
	vec2 p = vec2(UV.x - 0.5, (UV.y - 0.5) * aspect);
	float c = cos(turn);
	float s = sin(turn);
	vec2 uv = centre + vec2(c * p.x - s * p.y, s * p.x + c * p.y) * span;
	vec4 plan_colour = texture(plan, uv);
	// A cool cast, so the plan sits with the HUD's greys.
	plan_colour.rgb *= vec3(0.9, 0.96, 1.0);
	// Off the map (outside the plan's square, or a pixel with no ground):
	// thin red contour lines round its edge, a metre and a half apart,
	// fading out. The distance to the nearest ground is found by looking out
	// in eight directions, half a metre at a time.
	bool outside = uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0;
	if (outside || plan_colour.a < 0.05) {
		float nearest = 99.0;
		for (int step = 1; step <= 10; step++) {
			float reach = float(step) * 0.5;
			for (int k = 0; k < 8; k++) {
				float a = float(k) * 0.785398;
				vec2 probe = uv + vec2(cos(a), sin(a)) * reach * metre;
				bool inside = probe.x >= 0.0 && probe.y >= 0.0 && probe.x <= 1.0 && probe.y <= 1.0;
				if (inside && textureLod(plan, probe, 0.0).a > 0.5) {
					nearest = min(nearest, reach);
				}
			}
			if (nearest < 99.0) {
				break;
			}
		}
		float ring = step(0.5, nearest) * (1.0 - step(5.5, nearest));
		float line = ring * (1.0 - step(0.26, abs(mod(nearest + 0.25, 1.5) - 0.75)));
		plan_colour = vec4(bounds_colour.rgb, bounds_colour.a * line * (1.0 - nearest / 6.0));
	}
	// Quieten the plan towards the window's edge.
	float edge = max(abs(UV.x - 0.5), abs(UV.y - 0.5)) * 2.0;
	plan_colour.a *= opacity * mix(1.0, 0.7, smoothstep(0.75, 1.0, edge));
	if (jammed > 0.0) {
		vec2 cell = floor(UV * vec2(96.0, 96.0 * aspect)) + floor(time * 24.0) * vec2(7.0, 13.0);
		float n = hash(cell);
		float band = step(0.82, fract(UV.y * 3.0 - time * 1.7)) * 0.25;
		vec4 noise = vec4(vec3(0.55 + 0.45 * n), (0.35 + 0.45 * n + band) * opacity);
		plan_colour = mix(plan_colour, noise, jammed);
	}
	COLOR = plan_colour;
}
