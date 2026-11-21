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
//
// Damage (damage_amount > 0; tools/build_warehouse_wall_damage.py): the cast
// face's bug holes and bumps, spalls knocked out of panel feet and corners by
// forklifts and pallets (the scanned face broken back to its mesh), chipped
// joint arrises, spalls round some lifting inserts, grout repair patches, and
// hairline cracks rising from panel feet and impacts, running down from the
// roof line, and out from the top corners of doorways. Parallax
// occlusion mapping cuts them in; their slopes shade them as if lit from the
// high-bays above, since the baked lightmap carries no direction. They change
// only the colour: normal and roughness would cost the depth prepass too.
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
// The detail albedo is a baked shade (tools/build_warehouse_wall_damage.py):
// half the wall's tone times the light its relief gets from above (bug holes
// dark, bumps lit on top and shaded beneath), in one grey channel.
uniform bool detail_shade = false;
uniform float normal_strength = 0.5;
// Spalls and cracks; 0 leaves every panel whole (the insulated panels).
uniform sampler2D damage_tex : hint_default_black, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D crack_tex : hint_default_black, filter_linear_mipmap_anisotropic, repeat_disable;
uniform float damage_amount = 0.0;
// The broken scan's tile in metres, and how deep a spall reaches (to the mesh).
uniform float damage_size = 2.0;
uniform float damage_depth = 0.045;
// A crack strip's width and height in metres, and the crack's depth.
uniform vec2 crack_strip = vec2(0.5, 2.0);
uniform float crack_depth = 0.012;
// The top corners of the doorways in these walls, where cracks run out
// diagonally (tools/build_warehouse_wall_damage.py writes them from the layout):
// x along the wall, y the head height, z the wall's centre plane, w 0 for
// none or 1 + 2 * (wall runs along z) + (the crack leans toward +x along it).
uniform vec4 opening_corners[16];
uniform float parallax_distance = 14.0;
// Beyond this the damage fades out over its last fifth, and past it is skipped.
uniform float damage_distance = 45.0;

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

float noise1(float x) {
	float i = floor(x);
	float f = fract(x);
	return mix(hash12(vec2(i, 3.7)), hash12(vec2(i + 1.0, 3.7)), f * f * (3.0 - 2.0 * f));
}

// One panel's damage, chosen from its hashes. Positions are panel metres:
// x across the panel from its joint, y the world height. Plain fields, not
// arrays: arrays inside a returned struct do not survive Godot's compiler.
struct Damage {
	vec4 spall0;  // centre, radii (zero radii: none): left foot
	vec4 spall1;  // right foot
	vec4 spall2;  // a strike higher up
	vec4 spall3;  // round a lifting insert
	vec4 crack0;  // origin, scale (0: none), mirror (+1 or -1): from the foot
	vec4 crack1;  // from the roof line running down, or from a doorway's corner
	vec2 dirs;    // each crack's direction: 1 rising from its origin, -1 falling
	vec2 rot1;    // the second crack's lean (cos, sin)
	vec4 patch;   // a trowelled repair standing proud of the face
	vec2 strips;  // each crack's strip in the atlas
	float panel_w;
	vec2 shift;
	float texel;  // log2 of metres a screen pixel spans on the wall
};

float spall_mask(vec4 sp, vec2 q) {
	if (sp.z <= 0.0) {
		return 0.0;
	}
	// A lumpy outline: the radius wobbles round the centre (by 43% at most).
	vec2 o = (q - sp.xy) / sp.zw;
	if (max(abs(o.x), abs(o.y)) > 1.43) {
		return 0.0;
	}
	float a = atan(o.y, o.x);
	float seed = sp.x * 7.0 + sp.y * 3.0;
	float wob = 1.0 + 0.22 * sin(3.0 * a + seed) + 0.13 * sin(5.0 * a + seed * 1.7) + 0.08 * sin(9.0 * a + seed * 2.3);
	return 1.0 - smoothstep(0.3, 1.0, length(o) / wob);
}

// Where q falls in a crack's strip (0..1 both ways; outside: none), going
// `dir` (1 up from its origin, -1 down), leaning by `rot` (cos, sin).
vec2 crack_st(vec4 c, float dir, vec2 rot, vec2 q) {
	vec2 l = (q - c.xy) / max(c.z, 1e-3);
	l = vec2(rot.x * l.x - rot.y * l.y, rot.y * l.x + rot.x * l.y);
	l.x *= c.w;
	l.y *= dir;
	return vec2(l.x / crack_strip.x + 0.5, 1.0 - l.y / crack_strip.y);
}

bool in_strip(vec4 c, vec2 st) {
	return c.z > 0.0 && st.x > 0.0 && st.x < 1.0 && st.y > 0.0 && st.y < 1.0;
}

// A crack's depth (x) and weeping halo (y) at q.
vec2 crack_sample(Damage d, vec4 c, float strip, float dir, vec2 rot, vec2 q) {
	vec2 st = crack_st(c, dir, rot, q);
	if (!in_strip(c, st)) {
		return vec2(0.0);
	}
	vec2 uv = vec2((strip + st.x) * 0.25, st.y);
	// The strip is 256 texels across crack_strip.x metres, scaled by c.z.
	float lod = d.texel + log2(256.0 / (crack_strip.x * c.z));
	return textureLod(crack_tex, uv, lod).rg;
}

vec2 cracks_at(Damage d, vec2 q) {
	return max(crack_sample(d, d.crack0, d.strips.x, d.dirs.x, vec2(1.0, 0.0), q),
		crack_sample(d, d.crack1, d.strips.y, d.dirs.y, d.rot1, q));
}

// How far below the cast face q is broken, in spall depths (0 intact); `ck`
// gets the cracks' depth and halo there.
float damage_at(Damage d, vec2 q, out vec2 ck) {
	float mask = max(max(spall_mask(d.spall0, q), spall_mask(d.spall1, q)),
		max(spall_mask(d.spall2, q), spall_mask(d.spall3, q)));
	// Chipped arrises along some joints, in short runs.
	float dj = min(abs(q.x), abs(d.panel_w - q.x));
	if (dj < 0.035) {
		float side = q.x < d.panel_w * 0.5 ? 0.0 : 1.0;
		float run = smoothstep(0.72, 0.9, noise1(q.y * 4.5 + d.shift.x * 3.0 + side * 17.0)) * step(0.45, fract(d.shift.y + side * 0.37));
		mask = max(mask, (1.0 - smoothstep(0.008, 0.035, dj)) * run * 0.7);
	}
	float depth = 0.0;
	if (mask > 0.0) {
		vec2 g = (q + d.shift) / damage_size;
		float h = textureLod(damage_tex, g, d.texel + log2(1024.0 / damage_size)).r;
		// Shallow at the rim, down to the mesh only at the heart.
		depth = clamp(h * (0.6 + 0.8 * mask * mask) - (1.0 - mask) * 0.9, 0.0, 1.0);
	}
	ck = cracks_at(d, q);
	return max(depth, ck.x * crack_depth / damage_depth);
}

float damage_at(Damage d, vec2 q) {
	vec2 ck;
	return damage_at(d, q, ck);
}

// Four hashes at once (Dave Hoskins' hash42).
vec4 hash4(float panel, float face, float salt) {
	vec4 p4 = fract(vec4(panel, face * 7.0 + salt, panel, face * 7.0 + salt) * vec4(0.1031, 0.1030, 0.0973, 0.1099));
	p4 += dot(p4, p4.wzxy + 33.33);
	return fract((p4.xxyz + p4.yzzw) * p4.zywx);
}

// Four more from a panel's hash, cheaply: one hash per panel serves all its
// features.
vec4 rehash(vec4 b, float salt) {
	return fract(b.wxzy * (13.37 + salt * 1.618034) + b * 7.31 + salt * 0.1234);
}

// `u` and `plane` place the fragment along and across its wall, `axis` is 1
// for a wall running along z, and `start` is where its panel begins along u.
Damage panel_damage(float panel, float face, float panel_w, float texel,
		float u, float plane, float axis, float start, float y) {
	Damage d;
	d.panel_w = panel_w;
	d.texel = texel;
	vec4 base = hash4(panel, face, 0.0);
	d.shift = base.xy * 11.0;
	float a = damage_amount;
	// Feet of the joints: forklift tines and pallet corners.
	vec4 r = rehash(base, 1.0);
	d.spall0 = vec4(0.0);
	d.spall1 = vec4(0.0);
	if (r.x < 0.65 * a) {
		d.spall0 = vec4(0.06 + 0.5 * r.y, 0.05 + 0.6 * r.z, 0.3 + 0.5 * r.w, 0.2 + 0.35 * r.z);
	}
	r = rehash(base, 11.0);
	if (r.x < 0.65 * a) {
		d.spall1 = vec4(panel_w - 0.06 - 0.5 * r.y, 0.05 + 0.6 * r.z, 0.3 + 0.5 * r.w, 0.2 + 0.35 * r.z);
	}
	// A mast or load strike higher up the panel.
	r = rehash(base, 23.0);
	d.spall2 = vec4(0.0);
	if (r.x < 0.35 * a) {
		d.spall2 = vec4((0.2 + 0.6 * r.y) * panel_w, 0.9 + 0.9 * r.z, 0.2 + 0.35 * r.w, 0.15 + 0.2 * r.z);
	}
	// Round a lifting insert, broken out when the panel was stood up.
	r = rehash(base, 31.0);
	d.spall3 = vec4(0.0);
	if (r.x < 0.14 * a && insert_size.x > 0.0) {
		float col_i = r.y < 0.33 ? insert_columns.x : (r.y < 0.66 ? insert_columns.y : insert_columns.z);
		d.spall3 = vec4(col_i * panel_w + 0.03, insert_row0 + floor(r.z * 5.0) * insert_row_step - 0.05,
			0.16 + 0.12 * r.z, 0.13 + 0.1 * r.w);
	}
	// A crack rising from the foot, often from one of the strikes.
	r = rehash(base, 51.0);
	vec2 origin = vec2((0.15 + 0.7 * r.y) * panel_w, 0.0);
	if (d.spall2.z > 0.0 && r.w > 0.3 && r.w < 0.7) {
		origin = d.spall2.xy + vec2(0.0, d.spall2.w * 0.5);
	} else if (r.w < 0.5 && d.spall0.z > 0.0) {
		origin = d.spall0.xy + vec2(0.0, d.spall0.w * 0.5);
	} else if (r.w > 0.5 && d.spall1.z > 0.0) {
		origin = d.spall1.xy + vec2(0.0, d.spall1.w * 0.5);
	}
	d.crack0 = vec4(0.0);
	if (r.x < 0.75 * a) {
		d.crack0 = vec4(origin, 1.0 + 1.0 * r.z, r.w > 0.5 ? 1.0 : -1.0);
	}
	// One running down from the roof line, where the panel's top takes the deck.
	vec4 r2 = rehash(base, 61.0);
	d.crack1 = vec4(0.0);
	if (r2.x < 0.35 * a && y > roof_line - 5.0) {
		d.crack1 = vec4((0.15 + 0.7 * r2.y) * panel_w, roof_line, 1.0 + 1.2 * r2.z, r2.w > 0.5 ? 1.0 : -1.0);
	}
	d.strips = vec2(floor(r.z * 3.999), floor(r2.w * 3.999));
	d.dirs = vec2(1.0, -1.0);
	d.rot1 = vec2(1.0, 0.0);
	// The roof crack, the patch, and the doorway cracks are only made where
	// they can reach.
	// A doorway's top corner nearby takes over the second crack: the panel
	// cracks out from it diagonally, up and away from the opening.
	float best = 3.0;
	for (int i = 0; i < (y > 2.4 && y < 6.8 ? 16 : 0); i++) {
		vec4 oc = opening_corners[i];
		if (oc.w < 0.5) {
			break;
		}
		float du = abs(u - oc.x);
		if (du >= best || abs(plane - oc.z) >= 0.4) {
			continue;
		}
		float code = oc.w - 1.0;
		if (floor(code * 0.5) == axis && y > oc.y - 0.6 && y < oc.y + 3.2) {
			best = du;
			float side = mod(code, 2.0) > 0.5 ? 1.0 : -1.0;
			vec4 h = hash4(floor(oc.x * 3.0), oc.z, 97.0);
			float lean = side * (0.6 + 0.35 * h.x);
			d.crack1 = vec4(oc.x - start, oc.y, 0.8 + 0.6 * h.y, h.z > 0.5 ? 1.0 : -1.0);
			d.rot1 = vec2(cos(lean), sin(lean));
			d.dirs.y = 1.0;
			d.strips.y = floor(h.w * 3.999);
		}
	}
	// A repair: an old spall patched with grout, trowelled smooth and proud.
	vec4 r3 = rehash(base, 83.0);
	d.patch = vec4(0.0);
	if (r3.x < 0.3 * a && y < 2.8) {
		d.patch = vec4((0.2 + 0.6 * r3.y) * panel_w, 0.3 + 1.6 * r3.z, 0.25 + 0.35 * r3.w, 0.18 + 0.25 * r3.y);
	}
	return d;
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

	vec3 col = tint * (detail_shade ? det.r * 2.0 : mix(0.84, 1.14, clamp(lum * 2.2, 0.0, 1.0)));
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
		// Only two fires in the map: where neither the plume's widest sample
		// nor the slab under the wall has soot, skip the rest of it.
		float wide = textureLod(layout_b, luv, clamp(1.0 + world_pos.y * 0.75, 0.0, 6.0)).g
			* (1.0 + world_pos.y * 0.3);
		if (wide > 0.002 || lb.g > 0.002) {
			float lick = texture(masks, vec2(u * 0.7, world_pos.y * 0.16)).b;
			float lick2 = texture(masks, vec2(u * 2.1 + 0.3, world_pos.y * 0.45)).g;
			float plume = mix(lb.g, wide, smoothstep(0.0, 1.5, world_pos.y));
			float fade = 1.0 - smoothstep(1.5, 7.5, world_pos.y + (lick - 0.5) * 3.0);
			float soot = clamp(plume * fade * (0.55 + 0.9 * lick) * (0.8 + 0.4 * lick2) * 1.6, 0.0, 1.0);
			soot = soot * soot * (3.0 - 2.0 * soot);
			col = mix(col, vec3(0.022, 0.02, 0.018), soot * 0.95);
			wall_rough = mix(wall_rough, 0.95, soot);
		}
	}

	// Damage: march the view ray into the panel's spalls and cracks.
	vec3 tdir = n.x > n.z ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
	vec2 q0 = vec2(in_panel * panel_width, world_pos.y);
	vec2 q = q0;
	vec2 qgx = vec2(dFdx(u), dFdx(world_pos.y));
	vec2 qgy = vec2(dFdy(u), dFdy(world_pos.y));
	float spall = 0.0;
	vec2 ck = vec2(0.0);
	Damage dm;
	float dist = length(world_pos - CAMERA_POSITION_WORLD);
	float near = 1.0 - smoothstep(damage_distance * 0.8, damage_distance, dist);
	bool damaged = damage_amount > 0.0 && !top && near > 0.0;
	if (damaged) {
		dm = panel_damage(panel, face, panel_width, log2(max(max(length(qgx), length(qgy)), 1e-5)), u, n.x > n.z ? world_pos.x : world_pos.z,
			n.x > n.z ? 1.0 : 0.0, panel * panel_width + panel_offset, world_pos.y);
		float d = damage_at(dm, q, ck);
		if (d > 0.0 && dist < parallax_distance) {
			vec3 v = normalize(CAMERA_POSITION_WORLD - world_pos);
			float vn = max(dot(v, world_normal), 0.2);
			vec2 vt = vec2(dot(v, tdir), v.y);
			float steps = mix(12.0, 6.0, vn);
			vec2 stp = -vt / vn * damage_depth / steps;
			float layer = 0.0;
			for (int i = 0; i < 12; i++) {
				if (float(i) >= steps || layer >= d) {
					break;
				}
				q += stp;
				layer += 1.0 / steps;
				d = damage_at(dm, q);
			}
			// One refinement between the last two layers.
			vec2 prev = q - stp;
			float after = d - layer;
			float before = damage_at(dm, prev) - (layer - 1.0 / steps);
			q = mix(q, prev, clamp(after / (after - before + 1e-4), 0.0, 1.0));
			// Far off the relief flattens, so nothing pops at the range's edge.
			q = mix(q, q0, smoothstep(parallax_distance * 0.7, parallax_distance, dist));
			spall = damage_at(dm, q, ck);
		} else {
			spall = d;
		}
	}

	// Broken concrete: the scan's broken face, darker the deeper it goes, the
	// mesh rusting at the bottom, a fresh pale lip, and each slope shaded as if
	// lit from the high-bays; water weeps from the cracks and stains below.
	float inside = 0.0;
	if (damaged) {
		// The repair patch: paler and smoother grout, its raised rim lit along
		// the top and shadowed along the bottom.
		float pm = smoothstep(0.6, 0.75, spall_mask(dm.patch, q0)) * near;
		if (pm > 0.0 || spall_mask(dm.patch, q0 + vec2(0.0, 0.012)) > 0.6) {
			float rim_up = smoothstep(0.6, 0.75, spall_mask(dm.patch, q0 + vec2(0.0, 0.012))) - pm;
			// Trowel sweeps: arcs of slightly different tone across the grout.
			vec2 tq = (q0 - dm.patch.xy) * 9.0;
			float sweep = 0.5 + 0.5 * sin(length(tq + vec2(3.0, -7.0)) * 2.2 + tq.x * 0.4);
			// Its relief a third of the cast face's.
			float relief = detail_shade ? mix(1.0, det.r * 2.0 / 1.12, 0.35) : 1.0;
			vec3 grout = tint * vec3(1.1, 1.09, 1.05) * mix(0.93, 1.03, sweep) * relief;
			col = mix(col, grout, pm * 0.8);
			col *= 1.0 - 0.45 * rim_up;
			col *= 1.0 + 0.35 * max(-rim_up, 0.0);
		}
		float halo = ck.y * near;
		float h4 = hash12(vec2(panel * 5.0, face + 77.0));
		col *= 1.0 - 0.2 * halo;
		col = mix(col, vec3(0.62, 0.61, 0.57), halo * 0.22 * step(0.55, h4));
		if (spall > 0.0) {
			float e = 0.006;
			float sx = damage_at(dm, q + vec2(e, 0.0)) - damage_at(dm, q - vec2(e, 0.0));
			float sy = damage_at(dm, q + vec2(0.0, e)) - damage_at(dm, q - vec2(0.0, e));
			vec3 nl = normalize(vec3(vec2(sx, sy) * damage_depth / (2.0 * e), 1.0));
			inside = smoothstep(0.02, 0.14, spall) * near;
			vec4 dmg = textureGrad(damage_tex, (q + dm.shift) / damage_size, qgx / damage_size, qgy / damage_size);
			vec3 broken_col = tint * mix(0.62, 1.12, dmg.b) * vec3(1.0, 0.98, 0.94);
			broken_col *= 1.0 - 0.6 * sqrt(spall);
			broken_col = mix(broken_col, vec3(0.2, 0.09, 0.045), dmg.g * smoothstep(0.55, 0.85, spall));
			float lip = smoothstep(0.0, 0.04, spall) * (1.0 - smoothstep(0.04, 0.22, spall));
			col = mix(col, broken_col, inside);
			// A crack's open core is in deep shade.
			col *= 1.0 - 0.7 * smoothstep(0.2, 0.8, ck.x) * near;
			col *= 1.0 + 0.18 * lip * near;
			col *= mix(1.0, clamp(1.0 + 1.1 * nl.y - 0.3 * (1.0 - nl.z), 0.25, 1.7), near);
		}
	}

	col *= 1.0 - 0.6 * joint;
	col = mix(col, vec3(0.05, 0.05, 0.05), inserts * 0.85);
	ALBEDO = col;
	ROUGHNESS = clamp(wall_rough - inserts * 0.3, 0.05, 1.0);
	NORMAL_MAP = nmap;
	NORMAL_MAP_DEPTH = normal_strength;
}
