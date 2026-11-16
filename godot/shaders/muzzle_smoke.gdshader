// A soft puff of gun smoke, drawn on particles from the muzzle. Faces the
// camera; first-person weapons draw it with the viewmodel projection, so it
// leaves the muzzle where the weapon shows it. Fades with the particle's alpha.
shader_type spatial;
render_mode blend_mix, unshaded, cull_disabled, depth_draw_never, shadows_disabled;

#include "res://shaders/viewmodel_projection.gdshaderinc"

uniform vec4 tint : source_color = vec4(0.42, 0.42, 0.44, 1.0);
uniform float opacity = 0.5;
uniform bool viewmodel = true;

varying float puff_seed;

void vertex() {
	puff_seed = fract(dot(MODEL_MATRIX[3].xyz, vec3(12.9898, 78.233, 37.719)));
	// Billboard, keeping the particle's scale.
	float size = length(MODEL_MATRIX[0].xyz);
	vec4 centre = VIEW_MATRIX * vec4(MODEL_MATRIX[3].xyz, 1.0);
	vec4 view_position = centre + vec4(VERTEX.xy * size, 0.0, 0.0);
	if (viewmodel) {
		POSITION = viewmodel_position(PROJECTION_MATRIX, mat4(1.0), view_position.xyz);
	} else {
		POSITION = PROJECTION_MATRIX * view_position;
	}
}

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	return mix(
		mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x),
		mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x),
		f.y
	);
}

void fragment() {
	vec2 p = UV * 2.0 - 1.0;
	float wisps = noise(UV * 4.0 + puff_seed * 13.0) * 0.6 + noise(UV * 9.0 - puff_seed * 7.0) * 0.4;
	float puff = smoothstep(1.0, 0.2, length(p) + (wisps - 0.5) * 0.6);
	ALBEDO = tint.rgb;
	ALPHA = puff * opacity * COLOR.a;
}
