// Tilt-up concrete wall panels (Warehouse photo pass). World-space mapping
// along whichever horizontal axis the wall runs, so Cyclops wall blocks need
// no UVs. Draws vertical panel joints, the grid of dark lifting-insert
// pockets seen in the reference photos, low-frequency patching, faint leak
// streaks, and grime toward the slab.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

uniform sampler2D detail_albedo : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D detail_normal : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D masks : filter_linear_mipmap_anisotropic, repeat_enable;
uniform vec3 tint : source_color = vec3(0.41, 0.405, 0.38);
uniform float detail_size = 4.0;
uniform float mask_size = 8.0;
uniform float panel_width = 4.6;
uniform float panel_offset = 0.0;
uniform float joint_width = 0.02;
// Insert columns as fractions of the panel width, and the first row height and spacing.
uniform vec3 insert_columns = vec3(0.12, 0.5, 0.88);
uniform float insert_row0 = 1.1;
uniform float insert_row_step = 1.45;
uniform vec2 insert_size = vec2(0.055, 0.11);
uniform float grime_height = 0.7;
uniform float rough = 0.81;

varying vec3 world_pos;
varying vec3 world_normal;

void vertex() {
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
}

float box_mask(vec2 d, vec2 half_size, vec2 aa) {
	vec2 m = 1.0 - smoothstep(half_size, half_size + aa, abs(d));
	return m.x * m.y;
}

void fragment() {
	vec3 n = abs(world_normal);
	float u = n.x > n.z ? world_pos.z : world_pos.x;
	vec2 p = n.y > 0.7 ? world_pos.xz : vec2(u, world_pos.y);
	vec3 det = texture(detail_albedo, p / detail_size).rgb;
	float lum = dot(det, vec3(0.299, 0.587, 0.114));
	vec4 m = texture(masks, p / mask_size);

	float pu = (u - panel_offset) / panel_width;
	float in_panel = fract(pu);
	float dj = min(in_panel, 1.0 - in_panel) * panel_width;
	float aa_u = fwidth(u) + 1e-4;
	float joint = 1.0 - smoothstep(joint_width * 0.5, joint_width * 0.5 + aa_u, dj);

	// Lifting inserts: nearest column and row.
	float inserts = 0.0;
	float row = round((world_pos.y - insert_row0) / insert_row_step);
	float ry = world_pos.y - (insert_row0 + max(row, 0.0) * insert_row_step);
	vec2 aa = vec2(aa_u, fwidth(world_pos.y) + 1e-4);
	for (int i = 0; i < 3; i++) {
		float cx = (in_panel - insert_columns[i]) * panel_width;
		inserts = max(inserts, box_mask(vec2(cx, ry), insert_size * 0.5, aa));
	}
	if (n.y > 0.7 || insert_size.x <= 0.0) { inserts = 0.0; }
	if (n.y > 0.7) { joint = 0.0; }

	vec3 col = tint * mix(0.84, 1.14, clamp(lum * 2.2, 0.0, 1.0));
	col *= mix(0.88, 1.08, m.g);
	col *= 1.0 - 0.10 * smoothstep(0.55, 1.0, m.r) * (1.0 - smoothstep(4.0, 9.0, world_pos.y));
	col *= mix(0.72, 1.0, smoothstep(0.0, grime_height, world_pos.y));
	col *= 1.0 - 0.18 * m.b;
	col *= 1.0 - 0.6 * joint;
	col = mix(col, vec3(0.05, 0.05, 0.05), inserts * 0.85);
	ALBEDO = col;
	ROUGHNESS = rough - inserts * 0.3;
	NORMAL_MAP = texture(detail_normal, p / detail_size).xyz;
	NORMAL_MAP_DEPTH = 0.5;
}
