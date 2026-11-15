// First-person weapon surface. Draws the weapon with its own field of view and
// squeezes its depth just in front of the near plane, so it keeps the same
// on-screen size when the world camera zooms (ADS, sprint FOV) and never clips
// into walls. Technique adapted from the community viewmodel shaders, rewritten
// for Godot 4.3+ reverse-Z depth.
shader_type spatial;
render_mode cull_back, depth_draw_opaque;

#include "res://shaders/viewmodel_projection.gdshaderinc"

uniform vec4 albedo : source_color = vec4(1.0);
uniform sampler2D albedo_texture : source_color, filter_linear_mipmap_anisotropic, repeat_enable, hint_default_white;
uniform float roughness : hint_range(0.0, 1.0) = 0.6;
uniform float metallic : hint_range(0.0, 1.0) = 0.0;

void vertex() {
	POSITION = viewmodel_position(PROJECTION_MATRIX, MODELVIEW_MATRIX, VERTEX);
}

void fragment() {
	ALBEDO = albedo.rgb * texture(albedo_texture, UV).rgb;
	ROUGHNESS = roughness;
	METALLIC = metallic;
	SPECULAR = 0.5;
}
