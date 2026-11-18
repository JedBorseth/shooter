// One material for a whole character kit (soldier, first-person arms, or
// zombies), so every character of a team draws in one call per mesh.
//
// tools/build_character_assets.py bakes each kit into one texture atlas:
// albedo, a tangent-space normal map, and ORM (occlusion, roughness,
// metallic). Each vertex also carries its palette role and its part group
// (UV2 = (role + 0.5) / 16, (group + 0.5) / 16).
//
// - Tinted roles (cloth, gear, gloves, boots) store only shading detail in the
//   albedo, centred on 0.5; the team's palette colour gives the hue and value.
//   Other roles (skin, eyes, lenses, hardware) use the albedo as it is.
// - Roles in `hidden_roles` and part groups in the per-character `hidden_parts`
//   collapse to a point, shadows included: a zombie's lost helmet, or the
//   heads and kit a soldier is not wearing.
// - Per character (`variant`): skin tone, wear, and for zombies blood.
// - The first-person arms draw with a copy compiled with VIEWMODEL_KEY
//   (rust/game/src/viewmodel.rs: `first_person_shader`), which adds the
//   first-person key light the weapon gets; every other character compiles
//   without it and pays nothing.
shader_type spatial;
render_mode cull_back, diffuse_burley, specular_schlick_ggx;

#include "res://shaders/viewmodel_projection.gdshaderinc"
#ifdef VIEWMODEL_KEY
#include "res://shaders/viewmodel_key.gdshaderinc"
#endif

const int ROLE_COUNT = 16;
const int SKIN = 0;
const int VISOR = 7;
const int EYES = 11;

uniform vec4 palette[16] : source_color;
// Bit i set: role i is coloured by the palette (its albedo is detail only).
uniform int tinted_roles = 0;
// Bit i set: role i is not drawn.
uniform int hidden_roles = 0;
// Bit i set: role i glows a little in its own colour (team accents).
uniform int emissive_roles = 0;
uniform float emission_energy = 0.35;
// Bit i set: role i is bare skin (tone varies per character).
uniform int skin_roles = 1;
uniform bool viewmodel = false;

uniform sampler2D albedo_texture : source_color, filter_linear_mipmap_anisotropic, hint_default_white;
uniform sampler2D normal_texture : hint_normal, filter_linear_mipmap_anisotropic;
uniform sampler2D orm_texture : hint_default_white, filter_linear_mipmap_anisotropic;
uniform float normal_strength = 1.0;
// How much each character's skin tone wanders from the texture's.
uniform float skin_variation = 0.18;
// A faint rim in the team colour so silhouettes hold in the dark.
uniform vec3 rim_colour : source_color = vec3(0.0);
uniform float rim_energy = 0.0;
// 1 for zombies: sickly skin, grime, and blood.
uniform float zombie = 0.0;

// Per character, so a crowd does not look cloned.
instance uniform float variant = 0.0;
// Bit g set: part group g is not drawn (heads, masks, and kit variants).
instance uniform int hidden_parts = 0;

varying flat int role;

void vertex() {
	role = clamp(int(UV2.x * 16.0), 0, ROLE_COUNT - 1);
	int group = clamp(int(UV2.y * 16.0), 0, 15);
	if (((hidden_roles >> role) & 1) != 0 || ((hidden_parts >> group) & 1) != 0) {
		VERTEX = vec3(0.0);
	}
	// Writing POSITION anywhere makes Godot use it everywhere, so both branches set it.
	if (viewmodel) {
		POSITION = viewmodel_position(PROJECTION_MATRIX, MODELVIEW_MATRIX, VERTEX);
	} else {
		POSITION = PROJECTION_MATRIX * (MODELVIEW_MATRIX * vec4(VERTEX, 1.0));
	}
}

float hash(vec2 p) {
	vec3 p3 = fract(vec3(p.xyx) * 0.1031);
	p3 += dot(p3, p3.yzx + 33.33);
	return fract((p3.x + p3.y) * p3.z);
}

float noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}

void fragment() {
	vec3 tex = texture(albedo_texture, UV).rgb;
	vec3 orm = texture(orm_texture, UV).rgb;
	vec3 base;
	if (((tinted_roles >> role) & 1) != 0) {
		// Detail centred on 0.5 times the team colour, a little faded or darkened
		// per character so a squad's kit is not uniform.
		float wear = noise(UV * 24.0 + variant * 5.3);
		base = palette[role].rgb * tex * 2.0 * (0.92 + 0.16 * wear);
	} else {
		base = tex;
	}
	if (((skin_roles >> role) & 1) != 0) {
		// Per character: darker or lighter, warmer or cooler.
		float tone = fract(variant * 0.618) * 2.0 - 1.0;
		float warm = fract(variant * 0.377) * 2.0 - 1.0;
		base *= 1.0 + tone * skin_variation;
		base *= vec3(1.0 + warm * 0.04, 1.0, 1.0 - warm * 0.05);
		if (zombie > 0.0) {
			float rot = noise(UV * 40.0 + variant * 2.1);
			vec3 sick = mix(vec3(0.62, 0.7, 0.55), vec3(0.75, 0.66, 0.6), fract(variant * 0.813));
			base *= mix(sick, sick * 0.7, rot * 0.6);
		}
	}
	float rough = orm.g;
	float metal = orm.b;
	if (zombie > 0.0) {
		float splatter = noise(UV * 18.0 + vec2(variant * 3.1, variant * 1.7)) * 0.7
			+ noise(UV * 60.0 - vec2(variant)) * 0.3;
		float blood = smoothstep(0.62, 0.72, splatter) * zombie;
		base = mix(base, vec3(0.19, 0.02, 0.012), blood * 0.8);
		rough = mix(rough, 0.25, blood);
		// Grime over everything.
		base *= 0.85 + 0.15 * noise(UV * 9.0 + variant);
	}
	ALBEDO = base;
	AO = orm.r;
	AO_LIGHT_AFFECT = 0.6;
	ROUGHNESS = rough;
	METALLIC = metal;
	SPECULAR = role == EYES || role == VISOR ? 0.6 : 0.4;
	NORMAL_MAP = texture(normal_texture, UV).rgb;
	NORMAL_MAP_DEPTH = normal_strength;
	vec3 glow = vec3(0.0);
	if (((emissive_roles >> role) & 1) != 0) {
		glow = palette[role].rgb * emission_energy;
	}
	if (rim_energy > 0.0) {
		float rim = pow(1.0 - clamp(dot(NORMAL, VIEW), 0.0, 1.0), 6.0);
		glow += rim_colour * rim * rim_energy;
	}
#ifdef VIEWMODEL_KEY
	vec3 n = viewmodel_key_normal(NORMAL_MAP, normal_strength, NORMAL, TANGENT, BINORMAL);
	glow += viewmodel_key_light(n, VIEW, VIEW_MATRIX, base, rough, metal, orm.r);
#endif
	EMISSION = glow;
}
