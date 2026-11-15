// Red screen-edge vignette for low health and fresh hits.
shader_type canvas_item;

uniform float intensity : hint_range(0.0, 1.0) = 0.0;
uniform vec4 tint : source_color = vec4(0.6, 0.0, 0.0, 1.0);

void fragment() {
	vec2 centred = UV * 2.0 - 1.0;
	// Wider than tall, so the corners and sides darken first.
	float edge = length(centred * vec2(0.85, 1.0));
	float mask = smoothstep(1.25 - intensity * 0.75, 1.45, edge);
	COLOR = vec4(tint.rgb, mask * intensity * tint.a);
}
