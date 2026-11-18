// The hit flash drawn over a character (`SoldierRig::hit`): an additive
// overlay that, like shaders/character.gdshader, collapses the part groups
// this character does not wear, so unworn face coverings never flash.
shader_type spatial;
render_mode unshaded, blend_add, cull_back, depth_draw_never, shadows_disabled;

uniform vec4 colour : source_color = vec4(1.0, 0.25, 0.1, 0.0);

// Shared with shaders/character.gdshader and declared in the same order, so
// both read the same per-character slots: set once per character.
instance uniform float variant = 0.0;
instance uniform int hidden_parts = 0;

void vertex() {
	int group = clamp(int(UV2.y * 16.0), 0, 15);
	if (((hidden_parts >> group) & 1) != 0) {
		VERTEX = vec3(0.0);
	}
}

void fragment() {
	ALBEDO = colour.rgb;
	ALPHA = colour.a;
}
