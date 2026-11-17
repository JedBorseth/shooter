// Surface shader for level geometry: PBR texture sets projected in world space,
// so blockout boxes and modelled props need no UVs and textures never stretch
// when a box is resized.
//
// Cost is kept close to the old flat blockout shader: each pixel samples its
// textures once, from the one axis its normal faces most (not three blended
// projections). Large-scale brightness variation and grime toward the ground
// break up tiling with a few ALU ops and no extra textures.
//
// Materials live in godot/materials/world/ and are written by
// tools/build_textures.py.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

uniform vec4 tint : source_color = vec4(1.0);
uniform bool textured = true;
uniform sampler2D albedo_tex : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D normal_tex : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D orm_tex : hint_default_white, filter_linear_mipmap_anisotropic, repeat_enable;
// Metres per texture repeat.
uniform float tile_size = 2.0;
uniform float normal_strength : hint_range(0.0, 2.0) = 1.0;
uniform float roughness_scale : hint_range(0.0, 2.0) = 1.0;
uniform float metallic_scale : hint_range(0.0, 1.0) = 1.0;
uniform float specular : hint_range(0.0, 1.0) = 0.5;
// Large-scale brightness variation, as a fraction of the albedo.
uniform float variation : hint_range(0.0, 0.5) = 0.12;
// Darkening of walls toward floor_height, fading out over grime_height metres.
uniform float grime : hint_range(0.0, 1.0) = 0.3;
uniform float grime_height = 0.7;
uniform float floor_height = 0.0;

// Windows (skyline buildings): a grid of windows on every wall, each with a
// fake room behind it (interior mapping: the view ray is intersected with a box
// in the shader, so a whole building of rooms costs no geometry or textures).
// The glass reflects the sky through the engine's own specular light.
uniform bool windows = false;
// Window grid in metres: column spacing and storey height, from world y = 0.
uniform vec2 window_cell = vec2(2.5, 3.2);
// The glass inside a cell: x from window_rect.x to 1 - window_rect.x, and y
// (from the floor up) from window_rect.y to window_rect.z.
uniform vec3 window_rect = vec3(0.24, 0.3, 0.82);
uniform float window_frame = 0.07;
uniform vec4 frame_color : source_color = vec4(0.2, 0.2, 0.21, 1.0);
uniform float room_depth = 4.0;
// Brightness of the rooms: unlit rooms and the share of lit ones.
uniform float room_light = 0.35;
uniform float lit_rooms = 0.2;
// Storeys below this height get no windows (a plain ground floor).
uniform float window_min_y = 2.0;

varying vec3 world_position;
varying vec3 world_normal;

void vertex() {
	world_position = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	// The normal matrix, not the model matrix: props are rotated and scaled
	// unevenly, which would tilt their normals.
	world_normal = normalize(MODEL_NORMAL_MATRIX * NORMAL);
}

// Sine-free hash (Dave Hoskins), cheaper than fract(sin()) on most GPUs.
float hash(vec2 p) {
	vec3 p3 = fract(vec3(p.xyx) * 0.1031);
	p3 += dot(p3, p3.yzx + 33.33);
	return fract((p3.x + p3.y) * p3.z);
}

float value_noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	return mix(
		mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x),
		mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x),
		f.y
	);
}

// A fake room behind one window: the colour seen along the view ray, lit by the
// room itself. `p` is where the ray enters, in metres inside the cell (x right,
// y up, z into the building); `d` is the ray in the same axes.
vec3 room_colour(vec3 p, vec3 d, float seed, out float lit) {
	vec3 size = vec3(window_cell, room_depth);
	// Keep the ray off exactly zero on any axis.
	d = mix(d, vec3(1e-4), lessThan(abs(d), vec3(1e-4)));
	vec3 t3 = (step(vec3(0.0), d) * size - p) / d;
	float t = min(t3.x, min(t3.y, t3.z));
	vec3 h = p + d * t;
	vec3 wall = mix(vec3(0.62, 0.58, 0.5), vec3(0.75, 0.73, 0.7), hash(vec2(seed, 3.1)));
	wall *= mix(vec3(1.0), vec3(0.75, 0.85, 1.0), step(0.75, hash(vec2(seed, 7.7))));
	vec3 c;
	if (t == t3.y) {
		// Floor (carpet or boards) or ceiling.
		c = d.y < 0.0 ? mix(vec3(0.22, 0.17, 0.13), vec3(0.32, 0.3, 0.28), hash(vec2(seed, 1.3)))
				: vec3(0.82, 0.8, 0.76);
	} else if (t == t3.x) {
		c = wall * 0.8;
	} else {
		c = wall;
		// Something on the back wall: a door, a shelf, or a picture.
		vec2 q = h.xy / size.xy;
		float kind = hash(vec2(seed, 9.2));
		vec2 lo = kind < 0.4 ? vec2(0.55, 0.0) : vec2(0.25, 0.4);
		vec2 hi = kind < 0.4 ? vec2(0.8, 0.68) : vec2(0.6, 0.62);
		if (all(greaterThan(q, lo)) && all(lessThan(q, hi))) {
			c = kind < 0.4 ? vec3(0.35, 0.26, 0.18) : vec3(0.18, 0.22, 0.26);
		}
	}
	// Darker toward the back and into the corners, as a room lit from its window.
	c *= mix(1.0, 0.45, h.z / size.z);
	lit = step(1.0 - lit_rooms, hash(vec2(seed, 5.5)));
	c *= room_light * mix(0.7, 1.3, hash(vec2(seed, 2.4)));
	c = mix(c, c * vec3(2.6, 2.1, 1.5) + vec3(0.06, 0.04, 0.02), lit);
	return c;
}

void fragment() {
	vec3 n = normalize(world_normal);
	vec3 a = abs(n);
	// The projection axis: u and v directions in world space for this face.
	vec3 u_dir;
	vec3 v_dir;
	if (a.y >= a.x && a.y >= a.z) {
		u_dir = vec3(1.0, 0.0, 0.0);
		v_dir = vec3(0.0, 0.0, 1.0);
	} else if (a.x >= a.z) {
		u_dir = vec3(0.0, 0.0, -sign(n.x));
		v_dir = vec3(0.0, -1.0, 0.0);
	} else {
		u_dir = vec3(sign(n.z), 0.0, 0.0);
		v_dir = vec3(0.0, -1.0, 0.0);
	}
	vec2 uv = vec2(dot(world_position, u_dir), dot(world_position, v_dir)) / tile_size;

	vec3 albedo = tint.rgb;
	float ao = 1.0;
	float rough = 0.85;
	float metal = 0.0;
	vec3 surface_normal = n;
	if (textured) {
		albedo *= texture(albedo_tex, uv).rgb;
		vec3 orm = texture(orm_tex, uv).rgb;
		ao = orm.r;
		rough = orm.g;
		metal = orm.b;
		// Normal maps import as two-channel (RGTC) textures: rebuild z.
		vec3 nm;
		nm.xy = texture(normal_tex, uv).rg * 2.0 - 1.0;
		nm.z = sqrt(max(1.0 - dot(nm.xy, nm.xy), 0.0));
		nm.xy *= normal_strength;
		// Tangent along +u; OpenGL normal maps put +green toward -v (image up).
		vec3 t = normalize(u_dir - n * dot(n, u_dir));
		vec3 b = normalize(-v_dir - n * dot(n, -v_dir));
		surface_normal = normalize(t * nm.x + b * nm.y + n * nm.z);
	}

	// Two octaves of value noise over the floor plan and height: no two walls
	// or floor bays come out exactly alike.
	vec2 macro = world_position.xz * 0.11 + vec2(world_position.y * 0.07);
	float noise = value_noise(macro) * 0.65 + value_noise(macro * 3.7 + 17.0) * 0.35;
	albedo *= 1.0 + (noise - 0.5) * 2.0 * variation;
	rough = clamp(rough * roughness_scale + (noise - 0.5) * 0.08, 0.04, 1.0);

	// Grime: rain splash and dirt on walls near the ground.
	float near_floor = 1.0 - smoothstep(floor_height, floor_height + grime_height, world_position.y);
	float wall = 1.0 - a.y;
	albedo *= 1.0 - grime * 0.45 * near_floor * wall * (0.6 + 0.4 * noise);

	metal *= metallic_scale;
	float spec = specular;
	if (windows && a.y < 0.5 && world_position.y > window_min_y) {
		// Wall axes: x along the wall, y up, z into the building.
		vec3 wall_n = a.x > a.z ? vec3(sign(n.x), 0.0, 0.0) : vec3(0.0, 0.0, sign(n.z));
		vec3 x_dir = vec3(-wall_n.z, 0.0, wall_n.x);
		vec2 cell_pos = vec2(dot(world_position, x_dir), world_position.y) / window_cell;
		vec2 id = floor(cell_pos);
		vec2 f = cell_pos - id;
		// Antialiased masks for the glass and its frame, about a pixel wide.
		vec2 aa = fwidth(cell_pos) * 0.75 + 1e-5;
		vec2 lo = vec2(window_rect.x, window_rect.y);
		vec2 hi = vec2(1.0 - window_rect.x, window_rect.z);
		vec2 border = window_frame / window_cell;
		vec2 outer = smoothstep(lo - border - aa, lo - border + aa, f)
			* (1.0 - smoothstep(hi + border - aa, hi + border + aa, f));
		vec2 inner = smoothstep(lo - aa, lo + aa, f) * (1.0 - smoothstep(hi - aa, hi + aa, f));
		float frame_mask = outer.x * outer.y;
		float glass_mask = inner.x * inner.y;
		// A sill under each window, a little wider than it.
		float sill = smoothstep(lo.x - 2.0 * border.x - aa.x, lo.x - 2.0 * border.x + aa.x, f.x)
			* (1.0 - smoothstep(hi.x + 2.0 * border.x - aa.x, hi.x + 2.0 * border.x + aa.x, f.x))
			* smoothstep(lo.y - 2.5 * border.y - aa.y, lo.y - 2.5 * border.y + aa.y, f.y)
			* (1.0 - smoothstep(lo.y - border.y - aa.y, lo.y - border.y + aa.y, f.y));
		float seed = hash(id + floor(world_position.xz * 0.01) * 13.0 + wall_n.xz * 7.0);

		vec3 view = normalize(world_position - CAMERA_POSITION_WORLD);
		vec3 d = vec3(dot(view, x_dir), view.y, dot(view, -wall_n));
		vec3 p = vec3(f * window_cell, 0.0);
		float lit;
		vec3 room = room_colour(p, d, seed, lit);
		// Blinds or curtains drawn part way down some windows.
		float wv = (f.y - lo.y) / (hi.y - lo.y);
		float blind = hash(vec2(seed, 4.4));
		blind = blind < 0.6 ? 0.0 : (blind - 0.6) * 2.0;
		float slats = 0.85 + 0.15 * smoothstep(0.3, 0.5, abs(fract(wv * 28.0) - 0.5));
		vec3 blind_col = mix(vec3(0.78, 0.74, 0.66), vec3(0.6, 0.62, 0.6), hash(vec2(seed, 6.6)));
		float covered = step(1.0 - blind, wv);
		room = mix(room, blind_col * slats * room_light * mix(0.7, 2.6, lit), covered);
		// The wall's thickness shades the top and sides of the opening.
		float reveal = smoothstep(0.0, 0.12, (hi.y - f.y) * window_cell.y)
			* smoothstep(0.0, 0.06, min(f.x - lo.x, hi.x - f.x) * window_cell.x);
		room *= mix(0.55, 1.0, reveal);

		vec3 sill_col = vec3(0.62, 0.6, 0.57);
		albedo = mix(albedo, sill_col, sill * (1.0 - frame_mask));
		albedo = mix(albedo, frame_color.rgb, frame_mask);
		rough = mix(rough, 0.55, frame_mask);
		metal = mix(metal, 0.0, frame_mask);
		ao = mix(ao, 1.0, frame_mask);
		surface_normal = normalize(mix(surface_normal, n, frame_mask));
		// Glass: dark and smooth, so it shows the room and reflects the sky.
		albedo = mix(albedo, vec3(0.01), glass_mask);
		rough = mix(rough, 0.04, glass_mask);
		spec = mix(spec, 0.5, glass_mask);
		EMISSION = room * glass_mask;
	}

	ALBEDO = albedo;
	ROUGHNESS = rough;
	METALLIC = metal;
	SPECULAR = spec;
	AO = ao;
	AO_LIGHT_AFFECT = 0.25;
	NORMAL = normalize((VIEW_MATRIX * vec4(surface_normal, 0.0)).xyz);
}
