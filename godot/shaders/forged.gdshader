// The Zombies forge's mark on an upgraded weapon: a cold blue sheen that
// runs along the gun in bands and catches its edges, drawn over the weapon's
// own surfaces as a material overlay (WeaponView::set_forged). The first-
// person copy uses the viewmodel's projection, so it lies exactly on it.
shader_type spatial;
render_mode blend_add, unshaded, cull_back, depth_draw_never, shadows_disabled, fog_disabled;

#include "res://shaders/viewmodel_projection.gdshaderinc"

uniform bool viewmodel = false;
uniform vec3 tint : source_color = vec3(0.2, 0.55, 1.0);
uniform float strength = 1.0;

varying vec3 local;

void vertex() {
	local = VERTEX;
	if (viewmodel) {
		POSITION = viewmodel_position(PROJECTION_MATRIX, MODELVIEW_MATRIX, VERTEX);
	}
}

void fragment() {
	// Bands sliding down the length of the gun (its -z), slanted a little.
	float bands = sin(local.z * 18.0 + local.y * 7.0 + TIME * 3.0);
	float glint = pow(max(bands, 0.0), 10.0);
	float rim = pow(1.0 - clamp(dot(NORMAL, VIEW), 0.0, 1.0), 3.0);
	ALBEDO = tint * strength * (0.015 + 0.22 * glint + 0.1 * rim);
}
