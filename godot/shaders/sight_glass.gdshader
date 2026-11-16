// Reflex sight lens: coated glass that tints what is behind it a little, more
// toward the rim. Multiplied over the scene, so it only ever darkens and never
// washes out. Drawn with the viewmodel projection in first person, so it lines
// up with the rest of the weapon.
shader_type spatial;
render_mode blend_mul, cull_back, depth_draw_never, unshaded, shadows_disabled;

#include "res://shaders/viewmodel_projection.gdshaderinc"

uniform bool viewmodel = true;
// Transmission at the centre and at the rim of the lens.
uniform vec3 tint : source_color = vec3(0.94, 0.97, 1.0);
uniform vec3 coating : source_color = vec3(0.62, 0.8, 0.88);
// Lens radius in the mesh's units; the lens is centred on its local origin in XY.
uniform float radius = 0.0118;

varying vec2 lens;

void vertex() {
	lens = VERTEX.xy / radius;
	if (viewmodel) {
		POSITION = viewmodel_position(PROJECTION_MATRIX, MODELVIEW_MATRIX, VERTEX);
	} else {
		POSITION = PROJECTION_MATRIX * (MODELVIEW_MATRIX * vec4(VERTEX, 1.0));
	}
}

void fragment() {
	float rim = smoothstep(0.7, 1.0, length(lens));
	ALBEDO = mix(tint, coating, rim);
}
