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

varying vec3 world_position;
varying vec3 world_normal;

void vertex() {
	world_position = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
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

	ALBEDO = albedo;
	ROUGHNESS = rough;
	METALLIC = metal * metallic_scale;
	SPECULAR = specular;
	AO = ao;
	AO_LIGHT_AFFECT = 0.25;
	NORMAL = normalize((VIEW_MATRIX * vec4(surface_normal, 0.0)).xyz);
}
