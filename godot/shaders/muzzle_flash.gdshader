// Additive star-shaped muzzle flash, projected like the weapon it sits on.
shader_type spatial;
render_mode blend_add, unshaded, cull_disabled, depth_draw_never, shadows_disabled;

#include "res://shaders/viewmodel_projection.gdshaderinc"

uniform vec4 color : source_color = vec4(1.0, 0.72, 0.38, 1.0);
uniform float energy = 3.0;
uniform float spikes = 4.0;

void vertex() {
	POSITION = viewmodel_position(PROJECTION_MATRIX, MODELVIEW_MATRIX, VERTEX);
}

void fragment() {
	vec2 p = UV * 2.0 - 1.0;
	float radius = length(p);
	float angle = atan(p.y, p.x);
	float star = 0.35 + 0.65 * pow(abs(cos(angle * spikes * 0.5)), 8.0);
	float glow = smoothstep(star, 0.0, radius);
	float core = smoothstep(0.35, 0.0, radius);
	ALBEDO = color.rgb * energy * (glow + core * 2.0);
}
