// The world surface shader's body, shared by world.gdshader and
// world_windows.gdshader (which defines WORLD_WINDOWS for the skyline's
// windows, so every other surface skips that code entirely).
//
// On High (WORLD_DETAIL, set in both shaders), a fine detail normal (a shared
// noise texture, `world_detail_normal`) adds grain and roughness breakup up
// close, where the texture alone would blur. (A second, turned colour sample
// to hide the texture's repeat was tried and dropped: its colour no longer
// matched the relief that lit it.) The graphics preset compiles this in or out
// (GraphicsQuality.set_world_detail): code a uniform switches off still slows
// every pixel, and Low and Medium cannot spare it. A wet map (WORLD_WET, set
// as the map loads, and `world_wetness` from scripts/looks.gd) darkens its
// surfaces and makes them glossy, and collects puddles on the ground.


uniform vec4 tint : source_color = vec4(1.0);
// Large-scale brightness variation: tiling value noise, 16 cells across
// (assets/textures/noise/macro.png, a project shader global).
global uniform sampler2D world_noise : filter_linear_mipmap, repeat_enable;
// Smudges and wear for the untextured materials (assets/textures/noise/wear.png,
// ambientCG SurfaceImperfections008), so painted, plastic, and laminate
// surfaces are not flat colour.
global uniform sampler2D world_wear : filter_linear_mipmap, repeat_enable;
#if WORLD_WET
// 0 dry to 1 soaked, per map (scripts/looks.gd).
global uniform float world_wetness;
#endif
#if WORLD_DETAIL
global uniform sampler2D world_detail_normal : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
// The detail texture repeats this many times per metre.
const float DETAIL_SCALE = 1.9;
// The detail fades out between these distances from the camera, in metres.
const float DETAIL_NEAR = 5.0;
const float DETAIL_FAR = 11.0;
#endif
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
// Untextured materials only: how strongly the shared wear map marks the
// albedo and roughness (0 leaves them flat).
uniform float wear : hint_range(0.0, 1.0) = 0.0;
// Darkening of walls toward floor_height, fading out over grime_height metres.
uniform float grime : hint_range(0.0, 1.0) = 0.3;
uniform float grime_height = 0.7;
// Road dust (vehicle paint): low on vertical faces the albedo turns toward
// dust and the finish goes matt, in blotches.
uniform float dust : hint_range(0.0, 1.0) = 0.0;
// Breaks up visible tiling on big isotropic surfaces (asphalt, concrete,
// plaster): a second albedo sample, rotated and at another scale, blended in
// patches by the world noise. One extra texture fetch where it is on. Leave it
// off for anything with courses or a grid (brick, tile), which rotation breaks.
uniform float anti_tile : hint_range(0.0, 1.0) = 0.0;
uniform float floor_height = 0.0;

#ifdef WORLD_WINDOWS
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
#endif

// Slamcity's shared materials (tools/build_slamcity_city.py): many looks share
// one material and carry their tint in the vertex colour and their roughness
// scale (halved) in its alpha; plain colours carry their roughness itself
// (`vertex_roughness`).
uniform bool use_vertex_color = false;
uniform bool vertex_roughness = false;
// Stripes across the texture's u axis (awnings), `stripe_width` metres wide.
uniform float stripe_width = 0.0;
uniform vec4 stripe_color : source_color = vec4(1.0);
// A checkerboard on floors, `checker_size` metres a square.
uniform float checker_size = 0.0;
uniform vec4 checker_color : source_color = vec4(0.0, 0.0, 0.0, 1.0);

// Models that move (a turning sentry head, a bobbing pickup, a board torn
// off a window) project in their own space instead, so their textures stay
// put on them: `world_position` and `world_normal` are then the model's own,
// with its origin on the ground under it, and the result turns back into the
// world at the end.
uniform bool object_space = false;

varying vec3 world_position;
varying vec3 world_normal;

void vertex() {
	if (object_space) {
		world_position = VERTEX;
		world_normal = normalize(NORMAL);
	} else {
		world_position = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
		// The normal matrix, not the model matrix: props are rotated and
		// scaled unevenly, which would tilt their normals.
		world_normal = normalize(MODEL_NORMAL_MATRIX * NORMAL);
	}
}

#ifdef WORLD_WINDOWS
#include "res://shaders/rooms.gdshaderinc"
#endif

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
	if (use_vertex_color) {
		albedo *= COLOR.rgb;
	}
	if (stripe_width > 0.0) {
		float u_m = dot(world_position, u_dir);
		albedo = mix(albedo, stripe_color.rgb, step(1.0, mod(floor(u_m / stripe_width), 2.0)));
	}
	if (checker_size > 0.0 && a.y > 0.5) {
		vec2 cell = floor(world_position.xz / checker_size);
		albedo = mix(albedo, checker_color.rgb, mod(cell.x + cell.y, 2.0));
	}
	float ao = 1.0;
	float rough = 0.85;
	float metal = 0.0;
	vec3 surface_normal = n;
	if (textured) {
		// Plain samples with the hardware's own gradients: explicit-gradient
		// samples with anisotropic filtering cost about 0.4 ms more per frame
		// at 1080p on a GTX 1060.
		vec3 base = texture(albedo_tex, uv).rgb;
		if (anti_tile > 0.0) {
			vec2 uv2 = vec2(uv.y, -uv.x) * 0.63 + vec2(0.37, 0.71);
			float patches = texture(world_noise, world_position.xz * 0.021 + world_position.y * 0.013).r;
			base = mix(base, texture(albedo_tex, uv2).rgb, anti_tile * smoothstep(0.35, 0.65, patches));
		}
		vec3 orm = texture(orm_tex, uv).rgb;
		// Normal maps import as two-channel (RGTC) textures: rebuild z.
		vec2 nxy = texture(normal_tex, uv).rg * 2.0 - 1.0;
		albedo *= base;
		ao = orm.r;
		rough = orm.g;
		metal = orm.b;
		vec3 nm;
		nm.xy = nxy * normal_strength;
#if WORLD_DETAIL
		{
			// Close-up detail: fine grain in the normal and the roughness,
			// fading out before it could shimmer.
			float close = 1.0 - smoothstep(DETAIL_NEAR, DETAIL_FAR, length(VERTEX));
			vec2 d = texture(world_detail_normal, uv * tile_size * DETAIL_SCALE).rg * 2.0 - 1.0;
			d *= close;
			nm.xy += d * 0.3 * normal_strength;
			rough = clamp(rough + (d.x + d.y) * 0.1, 0.04, 1.0);
			albedo *= 1.0 + (d.x - d.y) * 0.04;
		}
#endif
		nm.z = sqrt(max(1.0 - dot(nm.xy, nm.xy), 0.0));
		// Tangent along +u; OpenGL normal maps put +green toward -v (image up).
		vec3 t = normalize(u_dir - n * dot(n, u_dir));
		vec3 b = normalize(-v_dir - n * dot(n, -v_dir));
		surface_normal = normalize(t * nm.x + b * nm.y + n * nm.z);
	}
	float worn = 0.0;
	if (!textured && wear > 0.0) {
		// 0.5 m per repeat, turned off the projection axes so it lines up with
		// nothing. Smudges only ever turn the surface rougher and a shade
		// darker: glossier patches mirror the sky in blotches, like marble.
		vec2 wuv = mat2(vec2(0.8, 0.6), vec2(-0.6, 0.8)) * (uv * tile_size) * 2.0;
		worn = (texture(world_wear, wuv).r - 0.54) * wear;
		albedo *= 1.0 - max(worn, 0.0) * 0.14;
	}

	// Two octaves of value noise over the floor plan and height: no two walls
	// or floor bays come out exactly alike.
	vec2 macro = world_position.xz * 0.11 + vec2(world_position.y * 0.07);
	float noise = texture(world_noise, macro / 16.0).r;
	albedo *= 1.0 + (noise - 0.5) * 2.0 * variation;
	if (vertex_roughness) {
		rough = COLOR.a;
	} else if (use_vertex_color) {
		// Shared textured materials: the look's roughness scale, halved.
		rough *= COLOR.a * 2.0;
	}
	rough = clamp(rough * roughness_scale + (noise - 0.5) * 0.08 + max(worn, 0.0) * 0.3, 0.04, 1.0);

	// Grime: rain splash and dirt on walls near the ground.
	float near_floor = 1.0 - smoothstep(floor_height, floor_height + grime_height, world_position.y);
	float wall = 1.0 - a.y;
	albedo *= 1.0 - grime * 0.45 * near_floor * wall * (0.6 + 0.4 * noise);
	if (dust > 0.0) {
		float fine = texture(world_noise, world_position.xz * 0.9 + world_position.y * 1.7).r;
		float d = dust * (1.0 - smoothstep(0.25, 1.1, world_position.y - floor_height)) * wall
			* smoothstep(0.25, 0.65, fine * 0.7 + noise * 0.5);
		albedo = mix(albedo, vec3(0.3, 0.27, 0.23), d * 0.75);
		rough = mix(rough, 0.9, d);
	}

#if WORLD_WET
	{
		// Wet: every surface darkens and turns glossy, the ground more than the
		// walls. Puddles fill the low patches of a broad noise on the ground:
		// dark mirrors for the sky and the lamps.
		float up = smoothstep(0.75, 0.95, n.y);
		float damp = world_wetness * mix(0.55, 1.0, up);
		albedo *= 1.0 - 0.35 * damp;
		rough = mix(rough, rough * 0.35 + 0.05, damp);
		float depth = texture(world_noise, world_position.xz * (0.32 / 16.0)).r;
		float puddle = smoothstep(0.6, 0.66, depth + world_wetness * 0.12) * up * step(0.5, world_wetness);
		albedo *= 1.0 - 0.45 * puddle;
		rough = mix(rough, 0.02, puddle);
		metal = mix(metal, 0.0, puddle);
		surface_normal = normalize(mix(surface_normal, n, puddle));
	}
#endif

	metal *= metallic_scale;
	float spec = specular;
#ifdef WORLD_WINDOWS
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
		vec3 room = room_colour(p, d, seed, vec3(window_cell, room_depth), room_light, lit_rooms, lit);
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
#endif

	ALBEDO = albedo;
	ROUGHNESS = rough;
	METALLIC = metal;
	SPECULAR = spec;
	AO = ao;
	// Texture AO is for light that comes from all around (sky, bounce); the
	// sun reaches into a crevice the texture cannot see, so it barely dims it.
	AO_LIGHT_AFFECT = 0.1;
	if (object_space) {
		surface_normal = normalize(MODEL_NORMAL_MATRIX * surface_normal);
	}
	NORMAL = normalize((VIEW_MATRIX * vec4(surface_normal, 0.0)).xyz);
}
