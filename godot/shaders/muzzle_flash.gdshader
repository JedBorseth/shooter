// Additive muzzle flash, projected like the weapon it sits on. Third-person
// weapons turn `viewmodel` off: the flash is then drawn in the world and turned
// to face the camera, so it reads from any side.
//
// `shape` 0 is the flash seen down the barrel, `shape` 1 the plume seen from the
// side (along the quad's +X). Each is a frame of the rendered flash atlas in
// `flipbook` (tools/build_muzzle_flash.py: 384-pixel cells, two rows per gun
// class, `flash_class`: 0 rifle, 1 SMG and pistol, 2 shotgun, 3 marksman
// rifle; set by the class's material, materials/muzzle_flash_<class>.tres),
// four variants from the hip and four smaller aimed ones per row, picked per
// shot by `seed` and switched to the aimed ones by `aimed`, both set per
// instance; its heat runs from `color` at the edges to `core_color` in the
// middle.
shader_type spatial;
render_mode blend_add, unshaded, cull_disabled, depth_draw_never, shadows_disabled;

#include "res://shaders/viewmodel_projection.gdshaderinc"

uniform vec4 color : source_color = vec4(1.0, 0.72, 0.38, 1.0);
uniform vec4 core_color : source_color = vec4(1.0, 0.95, 0.85, 1.0);
uniform float energy = 3.0;
uniform bool viewmodel = true;
uniform int shape = 0;
uniform int flash_class = 0;
uniform sampler2D flipbook : hint_default_black, filter_linear, repeat_disable;
/// How much bigger than its quad the first-person flash and plume draw
/// (Call of Duty's first-person flashes are oversized next to the gun; the
/// plume grows from the muzzle, not about its middle).
uniform float viewmodel_size = 1.5;
instance uniform float seed = 0.0;
instance uniform float aimed = 0.0;

void vertex() {
	if (viewmodel) {
		vec3 v = VERTEX;
		if (shape == 0) {
			v.xy *= viewmodel_size;
		} else {
			// The plume quad runs from the muzzle (its -X edge) down the
			// barrel: grow it away from the muzzle.
			float half_length = abs(VERTEX.x);
			v.x = (VERTEX.x + half_length) * viewmodel_size - half_length;
			v.y *= viewmodel_size;
		}
		POSITION = viewmodel_position(PROJECTION_MATRIX, MODELVIEW_MATRIX, v);
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
	// An 8 x 8 atlas: two rows (front, side) per class; columns 0 to 3 from
	// the hip, 4 to 7 aimed.
	float row = float(flash_class * 2 + shape);
	float column = mod(floor(seed), 4.0) + (aimed > 0.5 ? 4.0 : 0.0);
	vec2 cell = clamp(UV, vec2(0.003), vec2(0.997));
	// The atlas holds linear heat; lifted, the thin gas round the core
	// shows as well as the white-hot middle.
	float heat = pow(texture(flipbook, (vec2(column, row) + cell) * 0.125).r, 0.6);
	if (shape == 0) {
		// A soft, round hot core and a faint halo under the petals, so the
		// flash reads as a layered burst of gas rather than a drawn star.
		float r = length(cell - 0.5) * 2.0;
		heat = max(heat, exp(-r * r * 9.0) * 0.75 + exp(-r * r * 2.5) * 0.12);
	}
	vec3 tint = mix(color.rgb, core_color.rgb, smoothstep(0.45, 0.95, heat));
	ALBEDO = tint * energy * heat * 3.2;
}
