// Additive muzzle flash, projected like the weapon it sits on. Third-person
// weapons turn `viewmodel` off: the flash is then drawn in the world and turned
// to face the camera, so it reads from any side.
//
// `shape` 0 is the flash seen down the barrel, `shape` 1 the plume seen from the
// side (along the quad's +X). Each is a frame of the rendered flash atlas in
// `flipbook` (the firearms' material, materials/muzzle_flash.tres;
// tools/build_muzzle_flash.py: front stars, side plumes, and their smaller
// aimed versions, four of each), picked per shot by `seed` and switched to the
// aimed row by `aimed`, both set per instance; its heat runs from `color` at
// the edges to `core_color` in the middle.
shader_type spatial;
render_mode blend_add, unshaded, cull_disabled, depth_draw_never, shadows_disabled;

#include "res://shaders/viewmodel_projection.gdshaderinc"

uniform vec4 color : source_color = vec4(1.0, 0.72, 0.38, 1.0);
uniform vec4 core_color : source_color = vec4(1.0, 0.95, 0.85, 1.0);
uniform float energy = 3.0;
uniform bool viewmodel = true;
uniform int shape = 0;
uniform sampler2D flipbook : hint_default_black, filter_linear, repeat_disable;
instance uniform float seed = 0.0;
instance uniform float aimed = 0.0;

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

void fragment() {
	// A 4 x 4 atlas: rows front, side, aimed front, aimed side.
	float row = float(shape) + (aimed > 0.5 ? 2.0 : 0.0);
	float column = mod(floor(seed), 4.0);
	vec2 cell = clamp(UV, vec2(0.004), vec2(0.996));
	// The atlas holds linear heat; lifted, the thin gas round the core
	// shows as well as the white-hot middle.
	float heat = pow(texture(flipbook, (vec2(column, row) + cell) * 0.25).r, 0.6);
	vec3 tint = mix(color.rgb, core_color.rgb, smoothstep(0.45, 0.95, heat));
	ALBEDO = tint * energy * heat * 3.2;
}
