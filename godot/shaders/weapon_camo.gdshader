// A third-person or killcam weapon wearing a camo: the weapon's own PBR maps
// (as its StandardMaterial3D draws them) with the camo painted in
// (weapon_camo.gdshaderinc). It replaces the weapon's material, so a camo
// adds no draw call.
shader_type spatial;
render_mode cull_back, depth_draw_opaque;

#include "res://shaders/weapon_camo.gdshaderinc"

uniform vec4 albedo : source_color = vec4(1.0);
uniform sampler2D albedo_texture : source_color, filter_linear_mipmap_anisotropic, repeat_enable, hint_default_white;
uniform sampler2D normal_texture : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D orm_texture : hint_default_white, filter_linear_mipmap_anisotropic, repeat_enable;
uniform float ao_light_affect = 0.3;

void vertex() {
	CAMO_VERTEX
}

void fragment() {
	vec3 base = albedo.rgb * texture(albedo_texture, UV).rgb;
	vec3 orm = texture(orm_texture, UV).rgb;
	float roughness = orm.g;
	float metallic = orm.b;
	vec3 glow = vec3(0.0);
	camo_apply(base, roughness, metallic, glow, orm.b, NORMAL, VIEW, TIME);
	ALBEDO = base;
	AO = orm.r;
	AO_LIGHT_AFFECT = ao_light_affect;
	ROUGHNESS = roughness;
	METALLIC = metallic;
	NORMAL_MAP = texture(normal_texture, UV).rgb;
	EMISSION = glow;
}
