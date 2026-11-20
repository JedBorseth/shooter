// Warehouse slab and yard concrete. World-space XZ mapping, so Cyclops floor
// blocks need no UVs. Saw-cut control joints are drawn on a grid, and every
// slab between them is its own pour: its own tone, roughness, and patch of the
// detail and mask textures (shifted and turned per slab, the seams hidden in
// the joints), so the hall never shows a repeat. A few slabs are newer patch
// pours with dark sealant round them.
//
// Over that sit the hall's painted masks (tools/build_warehouse_floor_masks.py),
// read in world space: dust banked against walls and stock, forklift tyre
// tracks and the polish they leave, standing water (mirror-smooth, filling
// the slab's low spots first), oil, soot and scorch, extinguisher powder, damp,
// and broken slab. Where the slab is broken, parallax occlusion mapping cuts
// spalled pits and cracks into it (floor_damage.png): the relief is purely
// visual, so collision stays the flat slab and nothing catches a sprint or a
// slide, as Call of Duty keeps ground clutter out of player clip.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

uniform sampler2D detail_albedo : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D detail_normal : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D masks : filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D layout_a : hint_default_black, filter_linear_mipmap, repeat_disable;
uniform sampler2D layout_b : hint_default_black, filter_linear_mipmap, repeat_disable;
uniform sampler2D damage : hint_default_white, filter_linear_mipmap_anisotropic, repeat_enable;
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
// How much of the detail texture's own colour (its stains) shows through: the
// polished slab takes only its grain, the yard's weathered concrete its stains.
uniform float detail_colour = 0.0;
// Per-slab variation: tone spread, and how many slabs are patch pours.
uniform float slab_variation = 0.09;
uniform float patch_fraction = 0.1;
// The painted masks' world rectangle (x0, z0, width, depth).
uniform vec4 layout_rect = vec4(-18.0, -36.0, 64.0, 80.0);
uniform float grime_strength = 0.85;
// Broken slab: the damage tile's size in metres, the spalls' depth in metres.
uniform float damage_size = 2.0;
uniform float damage_depth = 0.035;
uniform float parallax_distance = 26.0;

varying vec3 world_pos;
varying float facing_up;

void vertex() {
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	facing_up = step(0.7, normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz).y);
}

float hash12(vec2 p) {
	vec3 p3 = fract(vec3(p.xyx) * 0.1031);
	p3 += dot(p3, p3.yzx + 33.33);
	return fract((p3.x + p3.y) * p3.z);
}

float joint_line(vec2 p, vec2 spacing, float width) {
	vec2 cell = (p - joint_offset) / spacing;
	vec2 d = abs(fract(cell + 0.5) - 0.5) * spacing;
	vec2 aa = fwidth(p) * 0.75 + 1e-4;
	vec2 l = 1.0 - smoothstep(vec2(width * 0.5), vec2(width * 0.5) + aa, d);
	return max(l.x, l.y);
}

// How deep the slab is broken at p (0 intact, 1 the spall's floor): the
// damage tile's pits, grown out from their deepest parts as the mask rises.
float broken(vec2 p, float mask) {
	float h = texture(damage, p / damage_size).r;
	return clamp((1.0 - h) * 2.2 - (1.0 - mask) * 1.7, 0.0, 1.0);
}

// A slab's own patch of a texture: shifted and turned a quarter turn at a
// time, sampled with the unshifted coordinate's derivatives so the seams at
// the joints never pick a blurry mip.
vec2 slab_turn(vec2 v, float turn) {
	return turn < 0.25 ? v : turn < 0.5 ? vec2(-v.y, v.x) : turn < 0.75 ? -v : vec2(v.y, -v.x);
}

// The quarter turn undone: brings a normal sampled in a turned patch back
// into the slab's own frame.
vec2 slab_unturn(vec2 v, float turn) {
	return turn < 0.25 ? v : turn < 0.5 ? vec2(v.y, -v.x) : turn < 0.75 ? -v : vec2(-v.y, v.x);
}

vec2 slab_uv(vec2 uv, float turn, vec2 shift) {
	return slab_turn(uv, turn) + shift;
}

void fragment() {
	vec2 p = world_pos.xz;
	vec2 luv = (p - layout_rect.xy) / layout_rect.zw;
	vec4 la = texture(layout_a, luv);
	// Slab edges (the yard's kerbs) take no spalls.
	la.a *= facing_up;
	vec4 lb = texture(layout_b, luv);
	float dist = length(world_pos - CAMERA_POSITION_WORLD);

	// Broken slab: march the view ray down into the spalls.
	float spall = 0.0;
	vec2 pp = p;
	if (la.a > 0.02 && dist < parallax_distance) {
		vec3 v = normalize(CAMERA_POSITION_WORLD - world_pos);
		float steps = mix(14.0, 6.0, abs(v.y));
		vec2 step_xz = -v.xz / max(v.y, 0.2) * damage_depth / steps;
		float layer = 0.0;
		float d = broken(pp, la.a);
		for (int i = 0; i < 14; i++) {
			if (float(i) >= steps || layer >= d) {
				break;
			}
			pp += step_xz;
			layer += 1.0 / steps;
			d = broken(pp, la.a);
		}
		// One refinement between the last two layers.
		vec2 prev = pp - step_xz;
		float after = d - layer;
		float before = broken(prev, la.a) - (layer - 1.0 / steps);
		float w = after / (after - before + 1e-4);
		pp = mix(pp, prev, clamp(w, 0.0, 1.0));
		// Far off the relief flattens into the plain damage sample, which is
		// what lies beyond the parallax range, so nothing pops at its edge.
		pp = mix(pp, p, smoothstep(parallax_distance * 0.7, parallax_distance, dist));
		spall = broken(pp, la.a);
	} else if (la.a > 0.02) {
		spall = broken(p, la.a);
	}

	// Each slab between the joints is its own pour.
	vec2 cell = floor((pp - joint_offset) / joint_spacing);
	float h1 = hash12(cell + 17.0);
	float h2 = hash12(cell * 1.7 + 3.1);
	float h3 = hash12(cell * 2.3 + 9.4);
	vec2 shift = vec2(h1, h2) * 37.0;
	float is_patch = step(1.0 - patch_fraction, h3);
	vec2 duv = pp / detail_size;
	vec2 suv = slab_uv(duv, h2, shift);
	// Gradients of the continuous world mapping, turned with the slab: a
	// difference across a joint never straddles two turns.
	vec2 gx = slab_turn(dFdx(p / detail_size), h2);
	vec2 gy = slab_turn(dFdy(p / detail_size), h2);
	vec3 det = textureGrad(detail_albedo, suv, gx, gy).rgb;
	vec3 nmap = textureGrad(detail_normal, suv, gx, gy).xyz;
	nmap.xy = slab_unturn(nmap.xy * 2.0 - 1.0, h2) * 0.5 + 0.5;
	float lum = dot(det, vec3(0.299, 0.587, 0.114));
	vec2 muv = slab_uv(pp / mask_size, h1, shift * 0.37);
	vec2 mgx = slab_turn(dFdx(p / mask_size), h1);
	vec2 mgy = slab_turn(dFdy(p / mask_size), h1);
	vec4 m = textureGrad(masks, muv, mgx, mgy);
	float macro = texture(masks, p / (mask_size * 3.7) + vec2(0.31, 0.17)).b;
	float joint = joint_line(pp, joint_spacing, joint_width * (0.7 + 0.8 * h1));
	// Patch pours: smoother, a shade off the old slab, ringed in dark sealant.
	vec2 in_cell = abs(fract((pp - joint_offset) / joint_spacing) - 0.5) * joint_spacing;
	float rim = is_patch * (1.0 - smoothstep(0.02, 0.05, joint_spacing.x * 0.5 - max(in_cell.x, in_cell.y * joint_spacing.x / joint_spacing.y)));

	vec3 col = tint * mix(0.82, 1.18, clamp(lum * 2.0, 0.0, 1.0));
	col = mix(col, det * tint * 2.2, detail_colour);
	col *= 1.0 + (h1 - 0.5) * 2.0 * slab_variation;
	col *= mix(vec3(1.0), vec3(1.0, 0.985, 0.95), h2);
	col = mix(col, col * vec3(1.1, 1.09, 1.06), is_patch);
	col *= mix(0.78, 1.16, m.b) * mix(0.82, 1.1, macro);
	col *= 1.0 - stain_strength * m.g * (1.0 - is_patch * 0.7);
	col *= 1.0 - crack_strength * m.r * (1.0 - is_patch);
	col *= 1.0 - 0.65 * max(joint, rim);
	float wear = clamp(m.g * 0.9 + (1.0 - macro) * 0.35 + m.r * 0.5, 0.0, 1.0);
	float rough = mix(rough_min, rough_max, wear) + joint * 0.3;
	rough = mix(rough, rough * 0.75 + 0.05, is_patch) + (h3 - 0.5) * 0.12;

	// Dust and grime banked against walls and stock: brown-grey and matte.
	// Breakup for the masks' edges: the slab's own grain, free to read.
	float edge = clamp(lum * 2.4 - 0.2, 0.0, 1.0);
	float grime = smoothstep(0.0, 1.0, la.r * (0.6 + 0.8 * edge)) * grime_strength;
	col = mix(col, col * vec3(0.55, 0.51, 0.45), grime);
	rough = mix(rough, 0.86, grime);
	// Tyre tracks: black rubber in the lines, the slab polished between.
	float tyre = smoothstep(0.3, 0.9, la.g);
	col *= 1.0 - 0.38 * tyre;
	rough = mix(rough, rough * 0.6, smoothstep(0.0, 0.35, la.g));
	// Damp: a darker, cooler bloom.
	col *= 1.0 - 0.28 * lb.a;
	rough = mix(rough, rough * 0.55, lb.a);
	// Soot and scorch: black, matte, streaked.
	float soot = smoothstep(0.0, 1.0, lb.g * (0.7 + 0.6 * edge));
	col = mix(col, vec3(0.025, 0.022, 0.02), soot * 0.92);
	rough = mix(rough, 0.95, soot);
	// Extinguisher powder: a pale chalky fan.
	float powder = smoothstep(0.1, 0.8, lb.b * (0.6 + 0.8 * edge));
	col = mix(col, vec3(0.78, 0.76, 0.7), powder * 0.85);
	rough = mix(rough, 0.95, powder);

	// Broken slab: rough and darker in the pits, aggregate showing.
	float inside = smoothstep(0.02, 0.15, spall);
	if (inside > 0.0) {
		float aggregate = texture(damage, pp / damage_size).g;
		// Dirt and grit collect in the pits: darker than the slab's face.
		vec3 pit = tint * mix(0.4, 0.78, aggregate) * vec3(0.98, 0.94, 0.88);
		col = mix(col, pit * (1.0 - 0.45 * spall), inside);
		rough = mix(rough, 0.92, inside);
	}

	// Oil: near black and glossy at its heart, a brown fringe.
	float oil = smoothstep(0.15, 0.5, lb.r + (edge - 0.5) * 0.25);
	col = mix(col, col * vec3(0.32, 0.29, 0.25), smoothstep(0.0, 0.3, lb.r) * 0.6);
	col = mix(col, vec3(0.012, 0.011, 0.01), oil * 0.9);
	rough = mix(rough, 0.1, oil);
	// Standing water: fills the slab's low spots (the spalls, the mottled
	// patches) first, darkens it, and lies mirror-flat.
	float low = (1.0 - m.b) * 0.35 + spall * 0.6;
	float water = smoothstep(0.42, 0.52, la.b + low * 0.35 + (edge - 0.5) * 0.2);
	float wet = smoothstep(0.1, 0.45, la.b + low * 0.2);
	col *= 1.0 - 0.3 * wet;
	rough = mix(rough, rough * 0.45, wet);
	col *= 1.0 - 0.25 * water;
	rough = mix(rough, 0.02, water);

	ALBEDO = col;
	ROUGHNESS = clamp(rough, 0.02, 1.0);
	SPECULAR = 0.5;

	// The spalls' own slope, from the damage depth either side: a pit's wall
	// faces the way the slab falls away into it.
	if (inside > 0.0) {
		float e = damage_size / 512.0 * 1.5;
		float dx = broken(pp + vec2(e, 0.0), la.a) - broken(pp - vec2(e, 0.0), la.a);
		float dz = broken(pp + vec2(0.0, e), la.a) - broken(pp - vec2(0.0, e), la.a);
		vec2 slope = vec2(dx, dz) * damage_depth / (2.0 * e);
		vec3 n_world = normalize(vec3(slope.x, 1.0, slope.y));
		NORMAL = normalize(mix(NORMAL, (VIEW_MATRIX * vec4(n_world, 0.0)).xyz, inside));
	}
	NORMAL_MAP = mix(nmap, vec3(0.5, 0.5, 1.0), water);
	NORMAL_MAP_DEPTH = normal_strength * (1.0 - 0.6 * inside);
}
