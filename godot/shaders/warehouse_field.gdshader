// Field scans in world metres. The painted mask holds green growth, worn
// ground, surviving concrete and standing water. Traffic direction and the
// demolished wing's slab grid come from the layout.
shader_type spatial;
render_mode diffuse_lambert, specular_schlick_ggx;

uniform sampler2D grass_albedo : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D grass_normal : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D track_albedo : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D slab_albedo : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D dirt_albedo : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D gravel_albedo : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D splat : filter_linear_mipmap, repeat_disable;
uniform vec4 layout_rect = vec4(-36.0, -38.0, 82.0, 82.0);
uniform vec3 grass_tint : source_color = vec3(0.86, 0.83, 0.71);
varying vec2 ground;

void vertex() {
	ground = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xz;
}

void fragment() {
	vec4 paint = texture(splat, (ground - layout_rect.xy) / layout_rect.zw);
	vec3 tuft = texture(grass_albedo, ground * 0.52).rgb;
	float straw_value = dot(tuft, vec3(0.30,0.58,0.12));
	vec3 straw = vec3(1.09,0.97,0.72) * straw_value;
	vec3 grass = mix(straw, tuft * vec3(0.67,0.98,0.69), smoothstep(0.14,0.90,paint.r)) * grass_tint;
	vec3 soil = texture(dirt_albedo, ground / 2.5).rgb * vec3(0.72,0.64,0.51);
	vec3 track = texture(track_albedo, ground / 2.1).rgb;
	vec3 gravel = texture(gravel_albedo, ground / 2.8).rgb * vec3(0.85,0.78,0.66);
	vec3 worn = mix(mix(soil, track, 0.38), gravel, smoothstep(0.18,0.87,paint.r) * 0.62);
	float bare = smoothstep(0.20,0.83,paint.g + straw_value*0.17);
	float slab_weight = smoothstep(0.22,0.85,paint.b + 0.11 * straw_value);
	// The former slabs are broad, grey planes, with cracks visible through
	// broken grass margins and joint lines authored in the mask.
	vec3 cast = texture(slab_albedo, ground / 4.1).rgb;
	vec3 slab = mix(cast, vec3(dot(cast, vec3(0.28, 0.60, 0.12))), 0.82)
		* mix(vec3(0.75,0.74,0.69), vec3(0.93,0.94,0.90), paint.r);
	vec3 col = mix(mix(grass, worn, bare), slab, slab_weight);
	float wet = smoothstep(0.35,0.86,paint.a);
	ALBEDO = mix(col, col * vec3(0.46,0.46,0.43), wet);
	NORMAL_MAP = texture(grass_normal, ground * 0.52).rgb;
	NORMAL_MAP_DEPTH = 0.68 * (1.0 - 0.74 * max(wet,slab_weight));
	ROUGHNESS = mix(mix(0.97,0.85,slab_weight), 0.24, wet * 0.80);
	SPECULAR = mix(0.21,0.48,wet);
}
