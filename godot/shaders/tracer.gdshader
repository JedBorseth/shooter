// Bullet tracer: a hot core with a soft glow around it, bright enough (HDR) that
// the glow pass blooms it. Drawn additively, so it reads against sky and shade.
shader_type spatial;
render_mode blend_add, unshaded, cull_disabled, depth_draw_never, shadows_disabled;

uniform vec4 color : source_color = vec4(1.0, 0.72, 0.38, 1.0);
uniform vec4 core_color : source_color = vec4(1.0, 0.95, 0.85, 1.0);
uniform float energy = 5.0;

void fragment() {
	// Box faces map UV across the streak's width; brightest down the middle.
	float across = abs(UV.x * 2.0 - 1.0);
	float core = smoothstep(0.45, 0.0, across);
	float glow = smoothstep(1.0, 0.0, across);
	ALBEDO = (color.rgb * glow * 0.6 + core_color.rgb * core) * energy;
}
