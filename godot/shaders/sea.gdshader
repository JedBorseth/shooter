shader_type spatial;
// Breakwater's harbour water (tools/breakwater_layout.py `basin`): a flat
// plane in the map mesh, given swell by two noise normal maps that scroll
// across each other in world space, a dark harbour green-grey (silted, not
// tropical) that lightens a little at grazing angles, and a glossy surface
// that mirrors the sky, more strongly toward the horizon (the Fresnel term
// of the engine's specular, helped by a darker body colour there). The foam
// and churned water along the walls are strips over it (sea_shore.gdshader).
// Cheap: two texture reads, no depth or screen reads.
render_mode cull_back, specular_schlick_ggx;

uniform vec3 deep_color : source_color = vec3(0.03, 0.065, 0.06);
uniform vec3 grazing_color : source_color = vec3(0.07, 0.11, 0.1);
uniform sampler2D swell : hint_normal, filter_linear_mipmap, repeat_enable;
uniform sampler2D chop : hint_normal, filter_linear_mipmap, repeat_enable;
uniform float swell_size = 34.0;
uniform float chop_size = 7.0;
uniform float strength : hint_range(0.0, 2.0) = 0.55;

varying vec3 world_pos;

void vertex() {
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	vec2 p = world_pos.xz;
	vec3 a = texture(swell, p / swell_size + vec2(TIME * 0.006, TIME * 0.004)).xyz * 2.0 - 1.0;
	vec3 b = texture(chop, p / chop_size + vec2(-TIME * 0.011, TIME * 0.009)).xyz * 2.0 - 1.0;
	vec3 n = normalize(vec3(a.xy + b.xy * 0.6, a.z * b.z));
	NORMAL_MAP = n * 0.5 + 0.5;
	NORMAL_MAP_DEPTH = strength;
	float facing = clamp(dot(NORMAL, VIEW), 0.0, 1.0);
	ALBEDO = mix(grazing_color, deep_color, facing);
	ROUGHNESS = 0.07;
	METALLIC = 0.0;
	SPECULAR = 0.5;
}
