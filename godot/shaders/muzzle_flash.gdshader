// Additive muzzle flash, projected like the weapon it sits on. Third-person
// weapons turn `viewmodel` off: the flash is then drawn in the world and turned
// to face the camera, so it reads from any side.
//
// `shape` 0 is the flash seen down the barrel: a hot core and ragged petals that
// change every shot (`seed`, set per instance). `shape` 1 is the plume seen from
// the side: a short cone along the quad's +X.
shader_type spatial;
render_mode blend_add, unshaded, cull_disabled, depth_draw_never, shadows_disabled;

#include "res://shaders/viewmodel_projection.gdshaderinc"

uniform vec4 color : source_color = vec4(1.0, 0.72, 0.38, 1.0);
uniform vec4 core_color : source_color = vec4(1.0, 0.95, 0.85, 1.0);
uniform float energy = 3.0;
uniform float spikes = 4.0;
uniform bool viewmodel = true;
uniform int shape = 0;
instance uniform float seed = 0.0;

void vertex() {
	if (viewmodel) {
		POSITION = viewmodel_position(PROJECTION_MATRIX, MODELVIEW_MATRIX, VERTEX);
	} else if (shape == 0) {
		float size = length(MODEL_MATRIX[0].xyz);
		vec4 centre = VIEW_MATRIX * vec4(MODEL_MATRIX[3].xyz, 1.0);
		POSITION = PROJECTION_MATRIX * (centre + vec4(VERTEX.xy * size, 0.0, 0.0));
	} else {
		// Writing POSITION anywhere makes Godot use it everywhere.
		POSITION = PROJECTION_MATRIX * (MODELVIEW_MATRIX * vec4(VERTEX, 1.0));
	}
}

float hash(float n) {
	return fract(sin(n) * 43758.5453);
}

void fragment() {
	vec2 p = UV * 2.0 - 1.0;
	float light = 0.0;
	if (shape == 0) {
		float radius = length(p);
		float angle = atan(p.y, p.x);
		// Ragged petals: each spike gets its own length from the shot's seed.
		float sector = floor((angle / TAU + 0.5) * spikes * 2.0);
		float reach = 0.55 + 0.45 * hash(sector * 7.13 + seed * 3.71);
		float star = 0.25 + reach * pow(abs(cos(angle * spikes * 0.5)), 6.0);
		float glow = smoothstep(star, 0.0, radius);
		float core = smoothstep(0.3, 0.0, radius);
		light = glow + core * 2.5;
		ALBEDO = mix(color.rgb, core_color.rgb, core) * energy * light;
	} else {
		// Along +X from the muzzle: bright at the base, tapering to a point.
		float along = UV.x;
		float width = (1.0 - along) * (0.55 + 0.25 * hash(seed + 1.7));
		float body = smoothstep(width, width * 0.2, abs(p.y));
		float fade = smoothstep(1.0, 0.15, along);
		light = body * fade;
		ALBEDO = mix(color.rgb, core_color.rgb, 1.0 - along) * energy * 0.8 * light;
	}
}
