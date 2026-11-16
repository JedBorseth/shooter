// One material for a whole character (body, gear, or first-person arms). Each
// vertex carries its palette role and rest-pose position (see
// write_merged_mesh in tools/build_character_assets.py), so a team needs one
// material instead of eleven, and a character draws in one call per mesh.
//
// Detail is computed in rest-pose space, so it sticks to the body while it
// animates: a fabric weave on cloth, mottling that differs per character
// (`variant`), dirt toward the feet, and for zombies a sickly skin tone and
// blood. Roles in `hidden_roles` collapse to a point (a zombie's lost helmet
// and goggles), shadows included.
shader_type spatial;
render_mode cull_back, diffuse_burley, specular_schlick_ggx;

#include "res://shaders/viewmodel_projection.gdshaderinc"

const int ROLE_COUNT = 11;
const int SKIN = 0;
const int UNIFORM = 1;
const int GLOVES = 2;
const int PANTS = 3;
const int BOOTS = 4;
const int MASK = 5;
const int VISOR = 7;
const int VEST = 8;
const int WEBBING = 9;

uniform vec4 palette[11] : source_color;
uniform float role_roughness[11];
// Bit i set: role i is not drawn.
uniform int hidden_roles = 0;
// Bit i set: role i glows a little in its own colour (team accents).
uniform int emissive_roles = 0;
uniform float emission_energy = 0.35;
uniform bool viewmodel = false;
// A greyscale fabric weave (the canvas texture set), repeated per metre.
uniform sampler2D weave : hint_default_white, filter_linear_mipmap, repeat_enable;
uniform float weave_scale = 9.0;
// 1 for zombies: rotting skin variation, grime, and blood.
uniform float zombie = 0.0;
// Per character, so a crowd does not look cloned.
instance uniform float variant = 0.0;

varying vec3 rest;
varying flat int role;

void vertex() {
	rest = vec3(UV, UV2.x);
	role = clamp(int(UV2.y * 16.0), 0, ROLE_COUNT - 1);
	if (((hidden_roles >> role) & 1) != 0) {
		VERTEX = vec3(0.0);
	}
	// Writing POSITION anywhere makes Godot use it everywhere, so both branches set it.
	if (viewmodel) {
		POSITION = viewmodel_position(PROJECTION_MATRIX, MODELVIEW_MATRIX, VERTEX);
	} else {
		POSITION = PROJECTION_MATRIX * (MODELVIEW_MATRIX * vec4(VERTEX, 1.0));
	}
}

float hash(vec3 p) {
	p = fract(p * 0.1031);
	p += dot(p, p.zyx + 31.32);
	return fract((p.x + p.y) * p.z);
}

float noise(vec3 p) {
	vec3 i = floor(p);
	vec3 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	return mix(
		mix(
			mix(hash(i), hash(i + vec3(1.0, 0.0, 0.0)), f.x),
			mix(hash(i + vec3(0.0, 1.0, 0.0)), hash(i + vec3(1.0, 1.0, 0.0)), f.x),
			f.y
		),
		mix(
			mix(hash(i + vec3(0.0, 0.0, 1.0)), hash(i + vec3(1.0, 0.0, 1.0)), f.x),
			mix(hash(i + vec3(0.0, 1.0, 1.0)), hash(i + vec3(1.0, 1.0, 1.0)), f.x),
			f.y
		),
		f.z
	);
}

void fragment() {
	vec3 base = palette[role].rgb;
	float rough = role_roughness[role];
	bool cloth = role == UNIFORM || role == PANTS || role == VEST || role == WEBBING || role == MASK;

	// Weave, projected along the rest-pose axis the surface faces most.
	vec3 facing = abs(cross(dFdx(rest), dFdy(rest)));
	vec2 plane = facing.x > facing.y && facing.x > facing.z
		? rest.zy
		: (facing.y > facing.z ? rest.xz : rest.xy);
	if (cloth) {
		float weave_value = texture(weave, plane * weave_scale).r;
		base *= mix(0.84, 1.12, weave_value);
		rough = clamp(rough + (weave_value - 0.5) * 0.12, 0.0, 1.0);
	}

	// Fading and wear, different on every character.
	float mottle = noise(rest * 3.5 + variant * 7.13);
	if (role != VISOR) {
		base *= 0.9 + 0.2 * mottle;
	}
	// Dirt and dust toward the feet.
	float feet = 1.0 - smoothstep(0.05, 0.7, rest.y);
	float dirt = role == BOOTS || role == PANTS ? 0.4 : 0.2;
	base *= 1.0 - dirt * feet * (0.6 + 0.4 * mottle);

	if (zombie > 0.0) {
		if (role == SKIN || role == MASK || role == GLOVES) {
			float tone = fract(variant * 0.618);
			base *= mix(vec3(0.82, 0.95, 0.78), vec3(1.08, 1.0, 0.92), tone);
		}
		float splatter = noise(rest * 5.0 + vec3(variant * 3.1)) * 0.7
			+ noise(rest * 13.0 - vec3(variant)) * 0.3;
		float blood = smoothstep(0.66, 0.74, splatter) * zombie;
		if (role != SKIN) {
			blood *= 1.0 + smoothstep(1.0, 1.4, rest.y);
		}
		base = mix(base, vec3(0.2, 0.025, 0.015), clamp(blood, 0.0, 1.0) * 0.7);
		rough = mix(rough, 0.3, clamp(blood, 0.0, 1.0));
	}

	ALBEDO = base;
	ROUGHNESS = rough;
	SPECULAR = role == VISOR ? 0.55 : 0.35;
	if (((emissive_roles >> role) & 1) != 0) {
		EMISSION = palette[role].rgb * emission_energy;
	}
}
