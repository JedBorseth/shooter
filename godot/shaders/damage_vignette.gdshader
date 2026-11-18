// Screen-edge vignette for low health and fresh hits. It multiplies the
// picture toward a dark blood red at the edges, so the middle of the screen
// (where the fight is) stays clear and the sky does not turn pink.
shader_type canvas_item;
render_mode blend_mul, unshaded;

uniform float intensity : hint_range(0.0, 1.0) = 0.0;
// What the edges are multiplied toward at full intensity.
uniform vec4 tint : source_color = vec4(0.55, 0.06, 0.04, 1.0);

void fragment() {
	vec2 centred = UV * 2.0 - 1.0;
	// Wider than tall, so the corners and sides darken first. The clear
	// middle shrinks a little as intensity rises, but never below most of
	// the screen.
	float edge = length(centred * vec2(0.85, 1.0));
	// The corners sit at about 1.31, so the ramp must end there to reach full
	// strength at all.
	float mask = smoothstep(1.05 - intensity * 0.45, 1.3, edge);
	// Reaches full strength at two thirds intensity, so a hurt player sees it
	// well before the last sliver of health.
	float amount = clamp(intensity * 1.5, 0.0, 1.0);
	COLOR = vec4(mix(vec3(1.0), tint.rgb, mask * amount * tint.a), 1.0);
}
