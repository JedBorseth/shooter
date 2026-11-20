// Tilt-up concrete wall panels (Warehouse). World-space mapping along
// whichever horizontal axis the wall runs, so Cyclops wall blocks need no UVs.
// Draws vertical panel joints, the grid of dark lifting-insert pockets seen in
// the reference photos, low-frequency patching, and grime toward the slab.
//
// Every panel is its own casting: its own tone and patch of the detail and
// mask textures (shifted per panel, the seams hidden in the joints). Ageing
// on top: rust running down from some of the inserts, rain streaks from the
// roof line where the flashing leaks, a damp band at the foot with a pale salt
// tide mark above it, and, read from the floor's painted masks at the wall's
// foot (tools/build_warehouse_floor_masks.py), dust banked against it, damp
// rising where water stands, and soot climbing the wall above a fire.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

uniform sampler2D detail_albedo : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D detail_normal : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D masks : filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D layout_a : hint_default_black, filter_linear_mipmap, repeat_disable;
uniform sampler2D layout_b : hint_default_black, filter_linear_mipmap, repeat_disable;
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
uniform float panel_variation = 0.07;
// Where the roof flashing sits (the walls' tops); leak streaks run down from it.
uniform float roof_line = 9.45;
uniform vec4 layout_rect = vec4(-18.0, -36.0, 64.0, 80.0);

varying vec3 world_pos;
varying vec3 world_normal;

void vertex() {
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
}

float hash12(vec2 p) {
	vec3 p3 = fract(vec3(p.xyx) * 0.1031);
	p3 += dot(p3, p3.yzx + 33.33);
	return fract((p3.x + p3.y) * p3.z);
}

float box_mask(vec2 d, vec2 half_size, vec2 aa) {
	vec2 m = 1.0 - smoothstep(half_size, half_size + aa, abs(d));
	return m.x * m.y;
}

void fragment() {
	vec3 n = abs(world_normal);
	bool top = n.y > 0.7;
	float u = n.x > n.z ? world_pos.z : world_pos.x;
	// Which face: each wall's two faces and each direction get their own panels.
	float face = (n.x > n.z ? 1.0 : 0.0) * 2.0 + (dot(world_normal, vec3(1.0, 0.0, 1.0)) > 0.0 ? 1.0 : 0.0);
	vec2 p = top ? world_pos.xz : vec2(u, world_pos.y);

	float pu = (u - panel_offset) / panel_width;
	float panel = floor(pu);
	float h1 = hash12(vec2(panel, face * 7.0 + 1.0));
	float h2 = hash12(vec2(panel * 1.3 + 5.0, face));
	float h3 = hash12(vec2(panel * 2.1, face * 3.0 + 11.0));
	vec2 shift = top ? vec2(0.0) : vec2(h1, h2) * 23.0;

	vec2 g = p / detail_size;
	vec3 det = textureGrad(detail_albedo, g + shift, dFdx(g), dFdy(g)).rgb;
	vec3 nmap = textureGrad(detail_normal, g + shift, dFdx(g), dFdy(g)).xyz;
	float lum = dot(det, vec3(0.299, 0.587, 0.114));
	vec2 mg = p / mask_size;
	vec4 m = textureGrad(masks, mg + shift * 0.41, dFdx(mg), dFdy(mg));

	float in_panel = fract(pu);
	float dj = min(in_panel, 1.0 - in_panel) * panel_width;
	float aa_u = fwidth(u) + 1e-4;
	float joint = 1.0 - smoothstep(joint_width * 0.5, joint_width * 0.5 + aa_u, dj);

	// Lifting inserts: nearest column and row.
	float inserts = 0.0;
	float rust = 0.0;
	float row = round((world_pos.y - insert_row0) / insert_row_step);
	float ry = world_pos.y - (insert_row0 + max(row, 0.0) * insert_row_step);
	vec2 aa = vec2(aa_u, fwidth(world_pos.y) + 1e-4);
	// The row above, for the rust running down from it.
	float row_above = max(ceil((world_pos.y - insert_row0) / insert_row_step), 0.0);
	float below = insert_row0 + row_above * insert_row_step - world_pos.y;
	for (int i = 0; i < 3; i++) {
		float cx = (in_panel - insert_columns[i]) * panel_width;
		inserts = max(inserts, box_mask(vec2(cx, ry), insert_size * 0.5, aa));
		// Some inserts weep rust: a streak widening and fading as it runs.
		float hr = hash12(vec2(panel * 3.0 + float(i), row_above + face * 13.0));
		float run = mix(0.4, 1.6, fract(hr * 7.3));
		float width = 0.018 + 0.02 * below / run;
		float streak = (1.0 - smoothstep(width * 0.4, width, abs(cx + 0.01 * sin(world_pos.y * 9.0 + hr * 6.0))))
			* (1.0 - smoothstep(0.0, run, below)) * step(0.0, below) * step(0.55, hr);
		rust = max(rust, streak);
	}
	if (top || insert_size.x <= 0.0) {
		inserts = 0.0;
		rust = 0.0;
	}
	if (top) {
		joint = 0.0;
	}

	vec3 col = tint * mix(0.84, 1.14, clamp(lum * 2.2, 0.0, 1.0));
	col *= 1.0 + (h1 - 0.5) * 2.0 * panel_variation;
	col *= mix(vec3(1.0), vec3(1.02, 1.0, 0.96), h2);
	col *= mix(0.88, 1.08, m.g);
	col *= 1.0 - 0.10 * smoothstep(0.55, 1.0, m.r) * (1.0 - smoothstep(4.0, 9.0, world_pos.y));
	col *= mix(0.72, 1.0, smoothstep(0.0, grime_height, world_pos.y));
	col *= 1.0 - 0.18 * m.b;
	float wall_rough = rough + (h3 - 0.5) * 0.08;

	if (!top) {
		// Rain streaks down from the roof line: narrow vertical runs, thick on
		// some panels and absent on others.
		vec2 sg = vec2(u * 2.3, world_pos.y * 0.07);
		float streaks = texture(masks, sg + vec2(h1 * 5.0, 0.0)).g;
		float leak = smoothstep(0.55, 0.85, streaks) * smoothstep(roof_line - 5.5 - h2 * 2.0, roof_line - 0.3, world_pos.y)
			* smoothstep(0.35, 0.75, h3);
		col *= 1.0 - 0.32 * leak;
		wall_rough = mix(wall_rough, 0.6, leak);
		// Rust from the inserts.
		col = mix(col, col * vec3(0.95, 0.62, 0.38), rust * 0.75);

		// What lies at the wall's foot, a little out from its face.
		vec2 foot = world_pos.xz + world_normal.xz * 0.4;
		vec2 luv = (foot - layout_rect.xy) / layout_rect.zw;
		vec4 la = texture(layout_a, luv);
		vec4 lb = texture(layout_b, luv);
		// Damp at the foot: higher where water stands; a salt line above it.
		float wobble = texture(masks, vec2(u * 0.11, 0.37)).b;
		float damp_h = 0.18 + 0.55 * wobble * h2 + 1.3 * max(lb.a, la.b);
		float damp = 1.0 - smoothstep(damp_h - 0.12, damp_h, world_pos.y);
		float salt = smoothstep(damp_h - 0.1, damp_h, world_pos.y) * (1.0 - smoothstep(damp_h, damp_h + 0.07, world_pos.y));
		col *= 1.0 - 0.25 * damp;
		wall_rough = mix(wall_rough, 0.55, damp);
		col = mix(col, vec3(0.6, 0.59, 0.56), salt * 0.18 * smoothstep(0.45, 0.85, m.r));
		// Dust banked against it.
		float dust = la.r * (1.0 - smoothstep(0.0, 0.35 + 0.3 * m.b, world_pos.y));
		col = mix(col, col * vec3(0.7, 0.66, 0.6), dust * 0.8);
		// Soot climbing from a fire at the foot: sharp where it starts, then
		// blurring wider and fading as it rises, its edges licked by noise.
		float lick = texture(masks, vec2(u * 0.7, world_pos.y * 0.16)).b;
		float lick2 = texture(masks, vec2(u * 2.1 + 0.3, world_pos.y * 0.45)).g;
		float base = texture(layout_b, luv).g;
		float wide = textureLod(layout_b, luv, clamp(1.0 + world_pos.y * 0.75, 0.0, 6.0)).g
			* (1.0 + world_pos.y * 0.3);
		float plume = mix(base, wide, smoothstep(0.0, 1.5, world_pos.y));
		float fade = 1.0 - smoothstep(1.5, 7.5, world_pos.y + (lick - 0.5) * 3.0);
		float soot = clamp(plume * fade * (0.55 + 0.9 * lick) * (0.8 + 0.4 * lick2) * 1.6, 0.0, 1.0);
		soot = soot * soot * (3.0 - 2.0 * soot);
		col = mix(col, vec3(0.022, 0.02, 0.018), soot * 0.95);
		wall_rough = mix(wall_rough, 0.95, soot);
	}

	col *= 1.0 - 0.6 * joint;
	col = mix(col, vec3(0.05, 0.05, 0.05), inserts * 0.85);
	ALBEDO = col;
	ROUGHNESS = clamp(wall_rough - inserts * 0.3, 0.05, 1.0);
	NORMAL_MAP = nmap;
	NORMAL_MAP_DEPTH = 0.5;
}
