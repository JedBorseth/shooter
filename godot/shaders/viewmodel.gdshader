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
// PBR maps from tools/build_weapons.py: a tangent-space normal map and an ORM
// map (R occlusion, G roughness, B metallic). Without them the flat values apply.
uniform bool use_maps = false;
uniform sampler2D normal_texture : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D orm_texture : hint_default_white, filter_linear_mipmap_anisotropic, repeat_enable;
uniform vec4 emission : source_color = vec4(0.0, 0.0, 0.0, 1.0);
uniform float emission_energy = 1.0;
// The first-person key light (scripts/viewmodel_light.gd): the direction toward
// it in world space and its strength, and its colour. Added here rather than as
// a light node, which would cost every lit pixel of the world.
global uniform vec4 viewmodel_key;
global uniform vec4 viewmodel_key_color;

void vertex() {
	POSITION = viewmodel_position(PROJECTION_MATRIX, MODELVIEW_MATRIX, VERTEX);
}

void fragment() {
	ALBEDO = albedo.rgb * texture(albedo_texture, UV).rgb;
	vec3 n = NORMAL;
	float occlusion = 1.0;
	if (use_maps) {
		vec3 orm = texture(orm_texture, UV).rgb;
		AO = orm.r;
		AO_LIGHT_AFFECT = 0.3;
		occlusion = orm.r;
		ROUGHNESS = orm.g;
		METALLIC = orm.b;
		NORMAL_MAP = texture(normal_texture, UV).rgb;
		// The same normal the renderer builds from NORMAL_MAP, for the key.
		vec3 nm;
		nm.xy = NORMAL_MAP.xy * 2.0 - 1.0;
		nm.z = sqrt(max(1.0 - dot(nm.xy, nm.xy), 0.0));
		n = normalize(TANGENT * nm.x + BINORMAL * nm.y + NORMAL * nm.z);
	} else {
		ROUGHNESS = roughness;
		METALLIC = metallic;
	}
	SPECULAR = 0.5;
	EMISSION = emission.rgb * emission_energy;
	if (viewmodel_key.w > 0.0) {
		// A shadowless key: Lambert diffuse and a normalised Blinn-Phong lobe,
		// in the renderer's units (a light of energy E lights white paper
		// facing it to E).
		vec3 l = normalize((VIEW_MATRIX * vec4(viewmodel_key.xyz, 0.0)).xyz);
		float ndl = max(dot(n, l), 0.0);
		float shine = exp2(11.0 * (1.0 - ROUGHNESS) + 1.0);
		float lobe = pow(max(dot(n, normalize(l + VIEW)), 0.0), shine) * (shine + 8.0) / 25.13;
		vec3 f0 = mix(vec3(0.04), ALBEDO, METALLIC);
		vec3 lit = ALBEDO * (1.0 - METALLIC) + f0 * lobe * 0.25;
		EMISSION += lit * ndl * occlusion * viewmodel_key_color.rgb * viewmodel_key.w;
	}
}
