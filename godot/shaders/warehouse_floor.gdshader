// Polished warehouse slab (Warehouse photo pass). World-space XZ mapping, so
// Cyclops floor blocks need no UVs. Saw-cut control joints are drawn
// procedurally on a grid; hairline cracks, stains, forklift scuffs, and slab
// mottle come from tools/build_warehouse_photo_textures.py (floor_masks.png).
// Low roughness with patchy wear gives the bright sheen under the high-bays
// seen in the reference photos.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

uniform sampler2D detail_albedo : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D detail_normal : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D masks : filter_linear_mipmap_anisotropic, repeat_enable;
uniform vec3 tint : source_color = vec3(0.39, 0.39, 0.38);
uniform float detail_size = 3.0;
uniform float mask_size = 16.0;
uniform vec2 joint_spacing = vec2(5.5, 5.5);
uniform vec2 joint_offset = vec2(0.0, 0.0);
uniform float joint_width = 0.009;
uniform float crack_strength = 0.45;
uniform float stain_strength = 0.55;
uniform float rough_min = 0.16;
uniform float rough_max = 0.62;
uniform float normal_strength = 0.12;

varying vec3 world_pos;

void vertex() {
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

float joint_line(vec2 p) {
	vec2 cell = (p - joint_offset) / joint_spacing;
	vec2 d = abs(fract(cell + 0.5) - 0.5) * joint_spacing;
	vec2 aa = fwidth(p) * 0.75 + 1e-4;
	vec2 l = 1.0 - smoothstep(vec2(joint_width * 0.5), vec2(joint_width * 0.5) + aa, d);
	return max(l.x, l.y);
}

void fragment() {
	vec2 p = world_pos.xz;
	vec3 det = texture(detail_albedo, p / detail_size).rgb;
	float lum = dot(det, vec3(0.299, 0.587, 0.114));
	vec4 m = texture(masks, p / mask_size);
	float macro = texture(masks, p / (mask_size * 3.7) + vec2(0.31, 0.17)).b;
	float joint = joint_line(p);

	vec3 col = tint * mix(0.82, 1.18, clamp(lum * 2.0, 0.0, 1.0));
	col *= mix(0.78, 1.16, m.b) * mix(0.82, 1.1, macro);
	col *= 1.0 - stain_strength * m.g;
	col *= 1.0 - crack_strength * m.r;
	col *= 1.0 - 0.65 * joint;
	ALBEDO = col;

	float wear = clamp(m.g * 0.9 + (1.0 - macro) * 0.35 + m.r * 0.5, 0.0, 1.0);
	ROUGHNESS = mix(rough_min, rough_max, wear) + joint * 0.3;
	SPECULAR = 0.5;
	NORMAL_MAP = texture(detail_normal, p / detail_size).xyz;
	NORMAL_MAP_DEPTH = normal_strength;
}
